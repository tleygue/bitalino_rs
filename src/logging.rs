use std::sync::Once;

use env_logger::Env;
use log::{LevelFilter, Log, Metadata, Record};
use once_cell::sync::OnceCell;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyModule, PyTuple};
use std::collections::HashMap;
use std::sync::Mutex;

static RUST_LOG_ONCE: Once = Once::new();
static PY_LOG_ONCE: Once = Once::new();
static PY_LOGGER: OnceCell<&'static PyLogger> = OnceCell::new();

fn env_level() -> LevelFilter {
    std::env::var("BITALINO_LOG")
        .or_else(|_| std::env::var("RUST_LOG"))
        .ok()
        .as_deref()
        .and_then(|v| v.parse::<LevelFilter>().ok())
        .unwrap_or(LevelFilter::Info)
}

fn level_to_str(level: LevelFilter) -> &'static str {
    match level {
        LevelFilter::Off => "off",
        LevelFilter::Error => "error",
        LevelFilter::Warn => "warn",
        LevelFilter::Info => "info",
        LevelFilter::Debug => "debug",
        LevelFilter::Trace => "trace",
    }
}

fn parse_level(input: Option<&str>) -> Option<LevelFilter> {
    input.and_then(|s| s.parse::<LevelFilter>().ok())
}

/// Initialize logging for Rust binaries (stderr formatter) based on `BITALINO_LOG`/`RUST_LOG`.
pub fn init_rust_logging() {
    let level = env_level();
    RUST_LOG_ONCE.call_once(|| {
        let env = Env::default().default_filter_or(level_to_str(level));
        env_logger::Builder::from_env(env)
            .format_timestamp_millis()
            .format_module_path(true)
            .format_target(true)
            .init();
    });
}

struct PyLogger {
    top_filter: Mutex<LevelFilter>,
    logging_mod: Py<PyModule>,
    cache: Mutex<HashMap<String, (LevelFilter, Py<PyAny>)>>, // target -> (effective_level, logger)
}

impl PyLogger {
    fn new(py: Python<'_>, top_filter: LevelFilter) -> PyResult<Self> {
        let logging = py.import("logging")?;
        Ok(Self {
            top_filter: Mutex::new(top_filter),
            logging_mod: logging.into(),
            cache: Mutex::new(HashMap::new()),
        })
    }

    fn map_level(level: log::Level) -> usize {
        match level {
            log::Level::Error => 40,
            log::Level::Warn => 30,
            log::Level::Info => 20,
            log::Level::Debug => 10,
            log::Level::Trace => 5,
        }
    }

    fn extract_max_level(logger: &pyo3::Bound<'_, PyAny>) -> PyResult<LevelFilter> {
        use log::Level::*;
        for l in &[Trace, Debug, Info, Warn, Error] {
            if Self::is_enabled_for(logger, *l)? {
                return Ok(l.to_level_filter());
            }
        }
        Ok(LevelFilter::Off)
    }

    fn is_enabled_for(logger: &pyo3::Bound<'_, PyAny>, level: log::Level) -> PyResult<bool> {
        let lvl = Self::map_level(level);
        logger.call_method1("isEnabledFor", (lvl,))?.is_truthy()
    }

    fn make_record(
        py: Python<'_>,
        logger: &pyo3::Bound<'_, PyAny>,
        target: &str,
        level: log::Level,
        record: &log::Record,
    ) -> PyResult<Py<PyAny>> {
        let lvl = Self::map_level(level);
        let none = py.None();
        let msg = format!("{}", record.args());
        logger
            .call_method1(
                "makeRecord",
                (
                    target,
                    lvl,
                    record.file(),
                    record.line().unwrap_or_default(),
                    msg,
                    PyTuple::empty(py),
                    &none, // exc_info
                    &none, // func
                    &none, // extra
                ),
            )
            .map(|obj| obj.into())
    }

    fn log_record(&self, record: &log::Record) {
        let target = record.target().replace("::", ".");

        Python::attach(|py| {
            let (enabled_level, logger_obj) = {
                let mut cache = self.cache.lock().unwrap();
                if let Some(entry) = cache.get(&target) {
                    (entry.0, entry.1.clone_ref(py))
                } else {
                    let logging = self.logging_mod.bind(py);
                    let logger = match logging
                        .getattr("getLogger")
                        .and_then(|f| f.call1((&target,)))
                    {
                        Ok(l) => l,
                        Err(e) => {
                            e.restore(py);
                            return;
                        }
                    };
                    let max_level = match Self::extract_max_level(&logger) {
                        Ok(l) => l,
                        Err(e) => {
                            e.restore(py);
                            LevelFilter::Off
                        }
                    };
                    let logger_owned = logger.unbind();
                    let cached = logger_owned.clone_ref(py);
                    cache.insert(target.clone(), (max_level, cached));
                    (max_level, logger_owned)
                }
            };

            let top = *self.top_filter.lock().unwrap();
            if record.level().to_level_filter() > enabled_level
                || record.level().to_level_filter() > top
            {
                return;
            }

            let py_logger = logger_obj.bind(py);
            match Self::make_record(py, &py_logger, &target, record.level(), record) {
                Ok(rec) => {
                    if let Err(e) = py_logger.call_method1("handle", (rec,)) {
                        e.restore(py);
                    }
                }
                Err(e) => e.restore(py),
            }
        });
    }
}

impl Log for PyLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        let top = *self.top_filter.lock().unwrap();
        metadata.level().to_level_filter() <= top
    }

    fn log(&self, record: &Record) {
        self.log_record(record);
    }

    fn flush(&self) {}
}

/// Initialize the Python-facing logging bridge so Rust logs flow into Python's `logging`.
/// Safe to call multiple times; a logger is installed on first call.
pub fn init_python_logging(py: Python<'_>) -> PyResult<()> {
    let level = env_level();
    PY_LOG_ONCE.call_once(|| match PyLogger::new(py, level) {
        Ok(logger) => {
            let leaked: &'static PyLogger = Box::leak(Box::new(logger));
            if log::set_logger(leaked).is_ok() {
                log::set_max_level(level);
                let _ = PY_LOGGER.set(leaked);
            }
        }
        Err(e) => e.restore(py),
    });
    Ok(())
}

/// Reset the cached per-target Python loggers (call after changing Python logging config).
pub fn reset_python_logging_cache() {
    if let Some(logger) = PY_LOGGER.get() {
        if let Ok(mut cache) = logger.cache.lock() {
            cache.clear();
        }
    }
}

/// Allow Python to set an explicit minimum level at runtime.
pub fn set_python_log_level(py: Python<'_>, level: LevelFilter) -> PyResult<()> {
    // Ensure initialization happened
    let _ = PY_LOGGER.get_or_try_init(|| {
        PyLogger::new(py, level).map(|logger| {
            let leaked: &'static mut PyLogger = Box::leak(Box::new(logger));
            leaked as &'static PyLogger
        })
    });

    if let Some(logger) = PY_LOGGER.get() {
        if let Ok(mut lf) = logger.top_filter.lock() {
            *lf = level;
        }
        reset_python_logging_cache();
    }
    log::set_max_level(level);
    Ok(())
}

/// Parse a string log level (or env fallback) and apply it to the Python bridge.
pub fn set_python_log_level_str(py: Python<'_>, level: Option<&str>) -> PyResult<()> {
    let lvl = parse_level(level).unwrap_or(env_level());
    set_python_log_level(py, lvl)
}
