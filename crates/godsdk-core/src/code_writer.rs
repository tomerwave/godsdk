use std::fmt::Display;

#[derive(Debug, Default)]
pub(crate) struct CodeWriter {
    output: String,
}

impl CodeWriter {
    pub(crate) fn from_lines<I, T>(lines: I) -> String
    where
        I: IntoIterator<Item = T>,
        T: Display,
    {
        let mut output = Self::default();
        output.lines(lines);
        output.finish()
    }

    pub(crate) fn from_parts<I, T>(parts: I) -> String
    where
        I: IntoIterator<Item = T>,
        T: AsRef<str>,
    {
        let mut output = Self::default();
        for part in parts {
            output.push(part.as_ref());
        }
        output.finish()
    }

    pub(crate) fn line(&mut self, value: impl Display) {
        self.output.push_str(&value.to_string());
        self.output.push('\n');
    }

    pub(crate) fn push(&mut self, value: &str) {
        self.output.push_str(value);
    }

    pub(crate) fn lines<I, T>(&mut self, lines: I)
    where
        I: IntoIterator<Item = T>,
        T: Display,
    {
        for line in lines {
            self.line(line);
        }
    }

    pub(crate) fn finish(self) -> String {
        self.output
    }
}
