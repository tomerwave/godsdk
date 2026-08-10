pub(crate) fn python_error_file_lines(imports: &[String], contracts: &str) -> Vec<String> {
    let mut lines = vec![
        "from __future__ import annotations".to_string(),
        String::new(),
        "from typing import TypeAlias".to_string(),
        String::new(),
    ];
    lines.extend(imports.iter().cloned());
    lines.extend([
        String::new(),
        "JsonValue: TypeAlias = None | bool | int | float | str | list[\"JsonValue\"] | dict[str, \"JsonValue\"]".to_string(),
        String::new(),
        "class SdkHttpError(Exception):".to_string(),
        "    status: int".to_string(),
        "    body: JsonValue".to_string(),
        String::new(),
        "    def __init__(self, status: int, body: JsonValue) -> None:".to_string(),
        "        super().__init__(f\"API returned HTTP {status}\")".to_string(),
        "        self.status = status".to_string(),
        "        self.body = body".to_string(),
        String::new(),
    ]);
    lines.extend(contracts.trim_end().lines().map(str::to_string));
    lines
}

pub(crate) fn python_error_contract_lines(
    name: &str,
    arms: &[String],
    subclasses: &[String],
) -> Vec<String> {
    let mut lines = vec![
        format!("class {name}(SdkHttpError):"),
        "    @classmethod".to_string(),
        format!("    def from_native(cls, status: int, body: JsonValue) -> {name}:"),
    ];
    lines.extend(arms.iter().cloned());
    lines.extend([
        "        return cls(status, body)".to_string(),
        String::new(),
    ]);
    lines.extend(subclasses.iter().cloned());
    lines
}
