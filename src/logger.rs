use chrono::Local;
use yansi::{Paint, Color};

/// A structure for using Logger.
///
/// By using new, you can get ready to use it.
///
/// ## Usage
///
/// ```rust
/// let logger = Logger::new(Option<&'static str>);
///
/// //Infomation
/// logger.pinfo("infomation");
///
/// //Caution
/// logger.pcaut("caution");
///
/// //Warning
/// logger.pwarn("warning");
///
/// //Error
/// logger.perr("error");
/// ```

#[derive(Debug, Clone)]
pub struct Logger { thread_name: Option<&'static str> }

#[allow(dead_code)]
impl Logger {
    pub fn new(thread_name: Option<&'static str>) -> Self {
        Self { thread_name }
    }

    fn p(&self, level: &str, level_color: Color, msg: impl Into<String>) {
        let thread = match self.thread_name {
            Some(value) => format!("[ {:<12} ] ", Paint::green(value)),
            None => "".to_string()
        };

        println!("[{}] [ {:^4} ] {}{}",
                 Local::now().format("%H:%M:%S - %m/%d"),
                 Paint::new(format!("{:<5}", level)).fg(level_color),
                 thread, msg.into())
    }

    pub fn info(&self, msg: impl Into<String>) {
        self.p("Info", Color::Cyan, msg);
    }

    pub fn caut(&self, msg: impl Into<String>) {
        self.p("Caut", Color::Yellow, msg);
    }

    pub fn warn(&self, msg: impl Into<String>) {
        self.p("Warn", Color::Magenta, msg);
    }

    pub fn error(&self, msg: impl Into<String>) {
        self.p("Error", Color::Red, msg);
    }

    pub fn debug(&self, msg: impl Into<String>) {
        self.p("Debug", Color::Magenta, msg);
    }

}