//! Gregorian names and time-of-day fields for the shared cell formatter.
use super::*;

fn marker(tokens: &[Token]) -> Option<(usize, bool)> {
    for (pattern, long) in [("AM/PM", true), ("A/P", false)] {
        if tokens.len() >= pattern.len()
            && tokens
                .iter()
                .zip(pattern.chars())
                .all(|(token, c)| code(token).is_some_and(|ch| ch.eq_ignore_ascii_case(&c)))
        {
            return Some((pattern.len(), long));
        }
    }
    None
}

pub(super) fn date_time(
    value: &CellValue,
    tokens: &[Token],
    culture: UiCulture,
) -> Result<String, &'static str> {
    let date = if let CellValue::Date(v) = value {
        Some(*v)
    } else {
        None
    };
    let time = if let CellValue::Time(v) = value {
        Some(*v)
    } else {
        None
    };
    let twelve_hour = (0..tokens.len()).any(|i| marker(&tokens[i..]).is_some());
    let day_number = tokens.iter().enumerate().any(|(i, t)| {
        code(t).is_some_and(|c| c.eq_ignore_ascii_case(&'d'))
            && (i == 0 || !code(&tokens[i - 1]).is_some_and(|c| c.eq_ignore_ascii_case(&'d')))
            && !tokens
                .get(i + 2)
                .and_then(code)
                .is_some_and(|c| c.eq_ignore_ascii_case(&'d'))
    });
    let mut result = String::new();
    let mut i = 0;
    while i < tokens.len() {
        if let Some((len, long)) = marker(&tokens[i..]) {
            let morning = time.is_none_or(|t| t.hour() < 12);
            let start = i + if morning {
                0
            } else if long {
                3
            } else {
                2
            };
            for token in &tokens[start..start + if long { 2 } else { 1 }] {
                result.push(code(token).expect("period marker"));
            }
            i += len;
            continue;
        }
        let Some(ch) = code(&tokens[i]) else {
            if let Token::Literal(c) = tokens[i] {
                result.push(c);
            }
            i += 1;
            continue;
        };
        if ch.is_ascii_alphabetic() {
            let field = ch.to_ascii_lowercase();
            let start = i;
            while i < tokens.len() && code(&tokens[i]).is_some_and(|c| c.eq_ignore_ascii_case(&ch))
            {
                i += 1;
            }
            let count = i - start;
            if field == 'm' && (3..=5).contains(&count) {
                let d = date.ok_or("Month name on a time value")?;
                let name = month_name(culture, d.month(), count == 3, day_number);
                if count == 5 {
                    result.push(name.chars().next().expect("month name"));
                } else {
                    result.push_str(name);
                }
                continue;
            }
            if field == 'd' && (3..=4).contains(&count) {
                let d = date.ok_or("Weekday name on a time value")?;
                let day = crate::DateValue {
                    year: i32::from(d.year()),
                    month: d.month(),
                    day: d.day(),
                }
                .weekday();
                result.push_str(weekday_name(culture, day, count == 3));
                continue;
            }
            let number = match field {
                'y' if count == 2 || count == 4 => {
                    let y = date.ok_or("Date field on a time value")?.year();
                    u32::from(if count == 2 { y % 100 } else { y })
                }
                'd' if count <= 2 => u32::from(date.ok_or("Date field on a time value")?.day()),
                'm' if count <= 2 => {
                    if let Some(t) = time {
                        u32::from(t.minute())
                    } else {
                        u32::from(date.expect("typed temporal value").month())
                    }
                }
                'h' if count <= 2 => {
                    let hour = time.map_or(0, |t| t.hour());
                    u32::from(if twelve_hour {
                        (hour + 11) % 12 + 1
                    } else {
                        hour
                    })
                }
                's' if count <= 2 => u32::from(time.map_or(0, |t| t.second())),
                _ => return Err("Unsupported date/time field"),
            };
            result.push_str(&format!("{number:0width$}", width = count));
        } else if ch == '0' && time.is_some() && i > 0 && code(&tokens[i - 1]) == Some('.') {
            let start = i;
            while i < tokens.len() && code(&tokens[i]) == Some('0') {
                i += 1;
            }
            if i - start > 9 {
                return Err("Fractional seconds exceed nanosecond precision");
            }
            let fraction = format!("{:09}", time.expect("time fractional seconds").nanosecond());
            result.push_str(&fraction[..i - start]);
        } else {
            result.push_str(&literal(&tokens[i..i + 1], None)?);
            i += 1;
        }
    }
    Ok(result)
}

fn month_name(culture: UiCulture, month: u8, short: bool, day: bool) -> &'static str {
    let names = match (culture, short, day) {
        (UiCulture::English, true, _) => [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ],
        (UiCulture::English, false, _) => [
            "January",
            "February",
            "March",
            "April",
            "May",
            "June",
            "July",
            "August",
            "September",
            "October",
            "November",
            "December",
        ],
        (UiCulture::Russian, true, _) => [
            "янв", "фев", "мар", "апр", "май", "июн", "июл", "авг", "сен", "окт", "ноя", "дек",
        ],
        (UiCulture::Russian, false, true) => [
            "января",
            "февраля",
            "марта",
            "апреля",
            "мая",
            "июня",
            "июля",
            "августа",
            "сентября",
            "октября",
            "ноября",
            "декабря",
        ],
        (UiCulture::Russian, false, false) => [
            "январь",
            "февраль",
            "март",
            "апрель",
            "май",
            "июнь",
            "июль",
            "август",
            "сентябрь",
            "октябрь",
            "ноябрь",
            "декабрь",
        ],
        (UiCulture::Arabic, _, _) => [
            "يناير",
            "فبراير",
            "مارس",
            "أبريل",
            "مايو",
            "يونيو",
            "يوليو",
            "أغسطس",
            "سبتمبر",
            "أكتوبر",
            "نوفمبر",
            "ديسمبر",
        ],
    };
    names[usize::from(month - 1)]
}
fn weekday_name(culture: UiCulture, day: u8, short: bool) -> &'static str {
    let names = match (culture, short) {
        (UiCulture::English, true) => ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"],
        (UiCulture::English, false) => [
            "Monday",
            "Tuesday",
            "Wednesday",
            "Thursday",
            "Friday",
            "Saturday",
            "Sunday",
        ],
        (UiCulture::Russian, true) => ["пн", "вт", "ср", "чт", "пт", "сб", "вс"],
        (UiCulture::Russian, false) => [
            "понедельник",
            "вторник",
            "среда",
            "четверг",
            "пятница",
            "суббота",
            "воскресенье",
        ],
        (UiCulture::Arabic, _) => [
            "الاثنين",
            "الثلاثاء",
            "الأربعاء",
            "الخميس",
            "الجمعة",
            "السبت",
            "الأحد",
        ],
    };
    names[usize::from(day)]
}
