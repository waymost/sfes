//! A problem found by a check, reported as `path:line: message`.

use std::fmt;

/// One problem found by a check.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct Finding {
    /// Path relative to the workspace root, with `/` separators.
    pub(crate) path: String,
    /// 1-based line, or 0 when the finding is about the whole file.
    pub(crate) line: usize,
    /// What is wrong and how to fix it.
    pub(crate) message: String,
}

impl Finding {
    pub(crate) fn new(path: impl fmt::Display, line: usize, message: impl fmt::Display) -> Self {
        Self {
            path: path.to_string(),
            line,
            message: message.to_string(),
        }
    }
}

impl fmt::Display for Finding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            path,
            line,
            message,
        } = self;
        if *line == 0 {
            write!(formatter, "{path}: {message}")
        } else {
            write!(formatter, "{path}:{line}: {message}")
        }
    }
}
