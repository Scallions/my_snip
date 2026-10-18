use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};

pub const MAX_LATEX_CHARS: usize = 16_000;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum OutputFormat {
    #[default]
    Latex,
    MdInline,
    MdBlock,
    WordLatex,
}

pub fn normalize_response(source: &str) -> AppResult<String> {
    let mut value = source.trim().trim_start_matches('\u{feff}').trim();
    if value.starts_with("```") {
        let newline = value.find('\n').ok_or_else(invalid_response)?;
        let language = value[3..newline].trim().to_lowercase();
        if !matches!(language.as_str(), "" | "latex" | "tex" | "math") || !value.ends_with("```") {
            return Err(invalid_response());
        }
        value = value[newline + 1..value.len() - 3].trim();
    }
    for (open, close) in [("$$", "$$"), ("\\[", "\\]"), ("\\(", "\\)"), ("$", "$")] {
        if value.starts_with(open) && value.ends_with(close) && value.len() > open.len() + close.len() {
            let interior = &value[open.len()..value.len() - close.len()];
            // Remove only a single outer pair, never two independent math regions.
            if open != "$" || !interior.contains('$') {
                value = interior.trim();
                break;
            }
        }
    }
    validate_latex(value)?;
    let lower = value.to_lowercase();
    if ["here is", "here's", "the formula", "the latex", "sorry", "i cannot", "i can't", "识别结果：", "识别结果:", "这是", "无法识别", "抱歉", "no_formula"]
        .iter().any(|prefix| lower.starts_with(prefix))
        || value.contains("```") || value.contains("<script") || value.contains("\\documentclass")
    {
        return Err(invalid_response());
    }
    Ok(value.to_owned())
}

pub fn validate_latex(value: &str) -> AppResult<()> {
    if value.trim().is_empty() {
        return Err(AppError::new("empty_result", "没有可复制的公式。", false));
    }
    if value.chars().count() > MAX_LATEX_CHARS {
        return Err(AppError::new("result_too_large", "公式过长，请缩小截图范围后重试。", false));
    }
    if value.chars().any(|c| c == '\0' || (c.is_control() && c != '\n' && c != '\r' && c != '\t')) {
        return Err(invalid_response());
    }
    Ok(())
}

pub fn export(value: &str, format: OutputFormat) -> AppResult<String> {
    validate_latex(value)?;
    let value = value.trim();
    match format {
        OutputFormat::Latex => Ok(value.into()),
        OutputFormat::MdInline => {
            if value.contains('\n') || value.contains("\\\\") {
                return Err(AppError::new("inline_multiline", "多行公式请使用 Markdown 独立公式格式。", false));
            }
            Ok(format!("${value}$"))
        }
        OutputFormat::MdBlock => Ok(format!("$$\n{value}\n$$")),
        OutputFormat::WordLatex => word_latex(value),
    }
}

fn word_latex(value: &str) -> AppResult<String> {
    // Word supports a different matrix input grammar. Do not use a broad
    // environment substitution: nested/unknown environments need manual review.
    let supported = ["matrix", "pmatrix", "bmatrix", "Bmatrix", "vmatrix", "Vmatrix"];
    let mut output = value.to_owned();
    for environment in supported {
        let start = format!("\\begin{{{environment}}}");
        let end = format!("\\end{{{environment}}}");
        if let Some(a) = output.find(&start) {
            let b = output[a + start.len()..].find(&end).map(|b| a + start.len() + b)
                .ok_or_else(|| AppError::new("word_syntax", "矩阵环境未闭合，请检查公式。", false))?;
            let inside = &output[a + start.len()..b];
            if inside.contains("\\begin{") {
                return Err(AppError::new("word_syntax", "嵌套矩阵请先复制纯 LaTeX，再在 Word 中调整。", false));
            }
            let matrix = format!("\\matrix{{{inside}}}");
            let replacement = match environment {
                "pmatrix" => format!("\\left({matrix}\\right)"),
                "bmatrix" => format!("\\left[{matrix}\\right]"),
                "Bmatrix" => format!("\\left\\{{{matrix}\\right\\}}"),
                "vmatrix" => format!("\\left|{matrix}\\right|"),
                "Vmatrix" => format!("\\left\\|{matrix}\\right\\|"),
                _ => matrix,
            };
            output.replace_range(a..b + end.len(), &replacement);
        }
    }
    if output.contains("\\begin{") || output.contains("\\end{") {
        return Err(AppError::new("word_environment", "这个公式环境尚未适配 Word，请切换纯 LaTeX，或在 Word 公式框中手动调整。", false));
    }
    Ok(output)
}

fn invalid_response() -> AppError {
    AppError::new("invalid_result", "模型没有返回可用的公式，请检查模型是否支持图片，或重新截图。", true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unwraps_only_outer_fences_and_preserves_commands() {
        assert_eq!(normalize_response("```latex\n\\[\\frac{x}{y}+\\text{cost \\$5}\\]\n```").unwrap(), "\\frac{x}{y}+\\text{cost \\$5}");
        assert_eq!(normalize_response("$x$ + $y$").unwrap(), "$x$ + $y$");
        assert!(normalize_response("```json\n{\"latex\":\"x\"}\n```").is_err());
        assert!(normalize_response("NO_FORMULA").is_err());
        assert!(normalize_response("Here is the LaTeX: x").is_err());
    }

    #[test]
    fn exports_consistently_without_mutating_formula() {
        let source = r"\frac{a}{b}";
        assert_eq!(export(source, OutputFormat::MdInline).unwrap(), r"$\frac{a}{b}$");
        assert_eq!(export(source, OutputFormat::MdBlock).unwrap(), "$$\n\\frac{a}{b}\n$$");
        assert!(export("x\\\\y", OutputFormat::MdInline).is_err());
        assert_eq!(export(r"\begin{pmatrix}a&b\\c&d\end{pmatrix}", OutputFormat::WordLatex).unwrap(), r"\left(\matrix{a&b\\c&d}\right)");
        assert!(export(r"\begin{aligned}x&=1\end{aligned}", OutputFormat::WordLatex).is_err());
    }
}
