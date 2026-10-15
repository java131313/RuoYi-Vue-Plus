/// PageQuery from plus-ui list requests. Fields stay String because
/// serde_urlencoded + flatten deserializes everything as strings.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PageQuery {
    pub page_num: String,
    pub page_size: String,
    pub order_by_column: String,
    pub is_asc: String,
}

impl Default for PageQuery {
    fn default() -> Self {
        PageQuery {
            page_num: "1".into(),
            page_size: String::new(),
            order_by_column: String::new(),
            is_asc: "asc".into(),
        }
    }
}

impl PageQuery {
    pub fn num(&self) -> i64 {
        self.page_num.parse().unwrap_or(1).max(1)
    }

    pub fn size(&self) -> i64 {
        let n: i64 = self.page_size.parse().unwrap_or(i64::MAX);
        if n <= 0 {
            i64::MAX
        } else {
            n
        }
    }

    /// ORDER BY clause with a column whitelist (defends against injection).
    pub fn order_by(&self, allowed: &[&str]) -> String {
        let col = self.order_by_column.trim();
        if col.is_empty() {
            return String::new();
        }
        let mut cols = Vec::new();
        for c in col.split(',') {
            let snake: String = c
                .chars()
                .map(|ch| {
                    if ch.is_ascii_uppercase() {
                        format!("_{}", ch.to_ascii_lowercase())
                    } else {
                        ch.to_string()
                    }
                })
                .collect();
            let snake = snake.trim_start_matches('_').to_string();
            if allowed.contains(&snake.as_str()) {
                cols.push(snake);
            }
        }
        if cols.is_empty() {
            return String::new();
        }
        let dir = if self.is_asc.eq_ignore_ascii_case("desc") || self.is_asc.eq_ignore_ascii_case("descending") {
            "DESC"
        } else {
            "ASC"
        };
        format!(" ORDER BY {} {}", cols.join(", "), dir)
    }

    pub fn limit_sql(&self) -> String {
        let size = self.size();
        if size == i64::MAX {
            String::new()
        } else {
            format!(" LIMIT {} OFFSET {}", size, (self.num() - 1) * size)
        }
    }
}
