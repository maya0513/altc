#[derive(Default)]
pub(super) struct Writer {
    text: String,
    indent: usize,
}

impl Writer {
    pub(super) fn line(&mut self, line: &str) {
        if !line.is_empty() {
            self.text.push_str(&"  ".repeat(self.indent));
            self.text.push_str(line);
        }
        self.text.push('\n');
    }

    pub(super) fn open(&mut self, line: &str) {
        self.line(line);
        self.indent += 1;
    }

    pub(super) fn close(&mut self, line: &str) {
        self.indent = self.indent.saturating_sub(1);
        self.line(line);
    }

    pub(super) fn reopen(&mut self, line: &str) {
        self.indent = self.indent.saturating_sub(1);
        self.line(line);
        self.indent += 1;
    }

    pub(super) fn finish(self) -> String {
        self.text
    }
}
