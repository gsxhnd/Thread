//! 展示层格式化：大小、耗时、时间与 HTTP 方法/状态码配色。

use gpui_kit::{App, Hsla, rgb};

/// HTTP 方法与状态码共用的语义色（明/暗两套）。
pub struct SemanticColors {
    pub get: Hsla,
    pub post: Hsla,
    pub put: Hsla,
    pub patch: Hsla,
    pub delete: Hsla,
    pub other: Hsla,
}

impl SemanticColors {
    /// 亮底（浅色主题）版本：压暗以保证文字对比度。
    pub fn light() -> Self {
        Self {
            get: rgb(0x1f8a54).into(),
            post: rgb(0xb4690e).into(),
            put: rgb(0x2563c9).into(),
            patch: rgb(0x8644cf).into(),
            delete: rgb(0xc2372f).into(),
            other: rgb(0x6b7280).into(),
        }
    }

    /// 暗底（深色主题）版本：提亮以拉开与背景的距离。
    pub fn dark() -> Self {
        Self {
            get: rgb(0x5ddba0).into(),
            post: rgb(0xe0a458).into(),
            put: rgb(0x7aa7f0).into(),
            patch: rgb(0xc79bf0).into(),
            delete: rgb(0xf0807a).into(),
            other: rgb(0x9ca3af).into(),
        }
    }

    /// 按当前主题的前景色亮度挑选：前景偏亮即深色主题。
    pub fn for_theme(theme: &gpui_kit::component::Theme) -> Self {
        if theme.foreground.l > 0.5 {
            Self::dark()
        } else {
            Self::light()
        }
    }
}

pub fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".to_string();
    }
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[0])
    } else if value < 10.0 {
        format!("{:.1} {}", value, UNITS[unit])
    } else {
        format!("{} {}", value.round(), UNITS[unit])
    }
}

pub fn format_duration(ms: f64) -> String {
    if !ms.is_finite() {
        return "-".to_string();
    }
    if ms < 1.0 {
        "<1 ms".to_string()
    } else if ms < 1000.0 {
        format!("{} ms", ms.round())
    } else {
        format!("{:.2} s", ms / 1000.0)
    }
}

pub fn format_relative_time(ms: i64, cx: &App) -> String {
    let now = chrono::Utc::now().timestamp_millis();
    let diff = (now - ms).max(0);
    if diff < 60_000 {
        crate::tr!(cx, "刚刚", "just now").to_string()
    } else if diff < 3_600_000 {
        let n = diff / 60_000;
        crate::tr!(cx, format!("{n} 分钟前"), format!("{n} min ago"))
    } else if diff < 86_400_000 {
        let n = diff / 3_600_000;
        crate::tr!(cx, format!("{n} 小时前"), format!("{n} hr ago"))
    } else if diff < 7 * 86_400_000 {
        let n = diff / 86_400_000;
        crate::tr!(cx, format!("{n} 天前"), format!("{n} days ago"))
    } else {
        chrono::DateTime::from_timestamp_millis(ms)
            .map(|dt| dt.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "-".to_string())
    }
}

/// 方法名对应的语义色（未知方法退到中性色）。
pub fn method_color(method: Option<&str>, colors: &SemanticColors) -> Hsla {
    match method.unwrap_or("").to_ascii_uppercase().as_str() {
        "GET" => colors.get,
        "POST" => colors.post,
        "PUT" => colors.put,
        "PATCH" => colors.patch,
        "DELETE" => colors.delete,
        _ => colors.other,
    }
}

/// 状态码对应的语义色。
pub fn status_color(status: Option<i64>, colors: &SemanticColors) -> Hsla {
    match status.unwrap_or(0) {
        200..300 => colors.get,
        300..400 => colors.put,
        400..500 => colors.post,
        500.. => colors.delete,
        _ => colors.other,
    }
}

pub const HTTP_METHODS: [&str; 8] = [
    "GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS", "TRACE",
];
