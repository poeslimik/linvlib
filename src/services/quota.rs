//! Asia/Seoul (UTC+9) calendar helpers for schedulers and status text.

use chrono::{FixedOffset, NaiveDate, Utc};

fn seoul_offset() -> FixedOffset {
    FixedOffset::east_opt(9 * 3600).expect("kst offset")
}

/// Asia/Seoul calendar date (UTC+9) as YYYY-MM-DD.
pub fn seoul_today() -> String {
    seoul_today_date().format("%Y-%m-%d").to_string()
}

pub fn seoul_today_date() -> NaiveDate {
    Utc::now().with_timezone(&seoul_offset()).date_naive()
}

pub fn seoul_now_display() -> String {
    Utc::now()
        .with_timezone(&seoul_offset())
        .format("%Y-%m-%d %H:%M:%S KST")
        .to_string()
}
