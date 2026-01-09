use chrono::{Local, NaiveDate};
use parking_lot::Mutex;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing_subscriber::fmt::MakeWriter;

/// 自定义日志写入器，支持按日期和大小滚动切割
pub struct RollingFileWriter {
    inner: Arc<Mutex<RollingFileWriterInner>>,
}

struct RollingFileWriterInner {
    directory: PathBuf,
    max_size_bytes: u64,
    current_date: NaiveDate,
    current_index: u32,
    current_size: u64,
    current_file: Option<File>,
}

impl RollingFileWriter {
    pub fn new(directory: impl AsRef<Path>, max_size_bytes: u64) -> io::Result<Self> {
        let directory = directory.as_ref().to_path_buf();
        fs::create_dir_all(&directory)?;

        let today = Local::now().date_naive();
        let (index, size) = Self::find_current_log_state(&directory, today)?;

        let mut inner = RollingFileWriterInner {
            directory,
            max_size_bytes,
            current_date: today,
            current_index: index,
            current_size: size,
            current_file: None,
        };

        inner.open_current_file()?;

        Ok(Self {
            inner: Arc::new(Mutex::new(inner)),
        })
    }

    fn find_current_log_state(directory: &Path, date: NaiveDate) -> io::Result<(u32, u64)> {
        let date_str = date.format("%Y-%m-%d").to_string();
        let mut max_index = 1u32;
        let mut current_size = 0u64;

        if let Ok(entries) = fs::read_dir(directory) {
            for entry in entries.flatten() {
                let file_name = entry.file_name();
                let name = file_name.to_string_lossy();

                if name.starts_with(&date_str) && name.ends_with(".log") {
                    if let Some(index_str) = name
                        .strip_prefix(&format!("{}-", date_str))
                        .and_then(|s| s.strip_suffix(".log"))
                    {
                        if let Ok(index) = index_str.parse::<u32>() {
                            if index >= max_index {
                                max_index = index;
                                if let Ok(metadata) = entry.metadata() {
                                    current_size = metadata.len();
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok((max_index, current_size))
    }
}

impl RollingFileWriterInner {
    fn get_log_filename(&self) -> String {
        format!(
            "{}-{}.log",
            self.current_date.format("%Y-%m-%d"),
            self.current_index
        )
    }

    fn open_current_file(&mut self) -> io::Result<()> {
        let path = self.directory.join(self.get_log_filename());
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        self.current_file = Some(file);
        Ok(())
    }

    fn check_rotation(&mut self) -> io::Result<()> {
        let today = Local::now().date_naive();

        if today != self.current_date {
            self.current_date = today;
            self.current_index = 1;
            self.current_size = 0;
            self.open_current_file()?;
        } else if self.current_size >= self.max_size_bytes {
            self.current_index += 1;
            self.current_size = 0;
            self.open_current_file()?;
        }

        Ok(())
    }

    fn write_log(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.check_rotation()?;

        if let Some(ref mut file) = self.current_file {
            let written = file.write(buf)?;
            self.current_size += written as u64;
            Ok(written)
        } else {
            Err(io::Error::other("No file opened"))
        }
    }
}

impl Clone for RollingFileWriter {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

pub struct RollingFileWriterGuard {
    inner: Arc<Mutex<RollingFileWriterInner>>,
}

impl Write for RollingFileWriterGuard {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.lock().write_log(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Some(ref mut file) = self.inner.lock().current_file {
            file.flush()
        } else {
            Ok(())
        }
    }
}

impl<'a> MakeWriter<'a> for RollingFileWriter {
    type Writer = RollingFileWriterGuard;

    fn make_writer(&'a self) -> Self::Writer {
        RollingFileWriterGuard {
            inner: Arc::clone(&self.inner),
        }
    }
}

/// 清理过期日志文件
pub async fn cleanup_old_logs(directory: impl AsRef<Path>, retention_days: u32) {
    let directory = directory.as_ref().to_path_buf();
    let cutoff_date = Local::now().date_naive() - chrono::Duration::days(retention_days as i64);

    tracing::info!("Cleaning logs older than {}", cutoff_date);

    if let Ok(entries) = fs::read_dir(&directory) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();

            if name.ends_with(".log") {
                if let Some(date_str) = name.get(0..10) {
                    if let Ok(file_date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                        if file_date < cutoff_date {
                            if let Err(e) = fs::remove_file(entry.path()) {
                                tracing::error!("Failed to remove log {:?}: {}", entry.path(), e);
                            } else {
                                tracing::info!("Removed old log: {:?}", entry.path());
                            }
                        }
                    }
                }
            }
        }
    }
}

/// 启动定时清理任务
pub fn start_cleanup_task(directory: String, retention_days: u32) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(86400));
        loop {
            interval.tick().await;
            cleanup_old_logs(&directory, retention_days).await;
        }
    });
}
