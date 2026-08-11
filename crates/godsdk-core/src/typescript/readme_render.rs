use crate::ApiIr;
use crate::code_writer::CodeWriter;

pub(super) fn render_readme(spec: &ApiIr) -> String {
    CodeWriter::from_lines([
        ["# ", &spec.title, " TypeScript SDK"].concat(),
        String::new(),
        "Install dependencies, then run `npm run test:native`. The command builds the Rust-backed napi-rs addon, starts a local mock API, and verifies runtime response validation with Zod.".to_string(),
    ])
}
