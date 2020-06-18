use {
    std::{
        convert::TryFrom,
        fmt
    },
    chrono::{
        LocalResult,
        prelude::*
    },
    chrono_tz::Tz,
    serde::Deserialize,
    crate::config::Config
};

enum Term {
    Oster,
    Sommer,
    Winter
}

struct EventId {
    term: Term,
    year: i32
}

impl EventId {
    fn current() -> EventId {
        //TODO check gefolge.org for closest date range (to add support for anschluss events etc)
        let today = Utc::today();
        EventId {
            term: match today.month() {
                2..=5 => Term::Oster,
                6..=9 => Term::Sommer,
                1 | 10..=12 => Term::Winter,
                _ => unreachable!()
            },
            year: today.year() - if today.month() < 3 { 1 } else { 0 }
        }
    }
}

impl fmt::Display for EventId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", match self.term {
            Term::Oster => "osil",
            Term::Sommer => "sosil",
            Term::Winter => "sil"
        }, self.year)
    }
}

/// A datetime that deserializes from a string that is either specified as UTC or without a timezone, as documented at <https://gefolge.org/wiki/event-json/meta>.
#[derive(Clone, Copy, Deserialize)]
#[serde(try_from = "String")]
struct HybridDateTime {
    inner: NaiveDateTime,
    aware: bool
}

impl HybridDateTime {
    fn with_timezone<Tz: TimeZone>(&self, tz: Tz) -> LocalResult<DateTime<Tz>> {
        if self.aware {
            LocalResult::Single(tz.from_utc_datetime(&self.inner))
        } else {
            tz.from_local_datetime(&self.inner)
        }
    }
}

impl TryFrom<String> for HybridDateTime {
    type Error = chrono::ParseError;

    fn try_from(s: String) -> Result<HybridDateTime, chrono::ParseError> {
        if let Ok(inner) = NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%SZ") {
            Ok(HybridDateTime { inner, aware: true })
        } else {
            NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S").map(|inner| HybridDateTime { inner, aware: false })
        }
    }
}

#[derive(Clone, Copy, Deserialize)]
struct Location {
    timezone: Tz
}

#[derive(Deserialize)]
pub(crate) struct Event {
    end: Option<HybridDateTime>,
    location: Option<Location>,
    start: Option<HybridDateTime>,
    timezone: Option<Tz>
}

impl Event {
    pub(crate) async fn current(config: &Config, client: &reqwest::Client) -> Result<Option<Event>, reqwest::Error> {
        let response = client.get(&format!("https://gefolge.org/api/event/{}/overview.json", EventId::current()))
            .basic_auth("api", Some(&config.api_key))
            .send().await?;
        Ok(if let Ok(response) = response.error_for_status() {
            Some(response.json().await?)
        } else {
            None
        })
    }

    pub(crate) fn end(&self) -> LocalResult<DateTime<Tz>> {
        self.end.map_or(LocalResult::None, |dt| dt.with_timezone(self.timezone()))
    }

    pub(crate) fn start(&self) -> LocalResult<DateTime<Tz>> {
        self.start.map_or(LocalResult::None, |dt| dt.with_timezone(self.timezone()))
    }

    pub(crate) fn timezone(&self) -> Tz {
        self.timezone.unwrap_or(self.location.map_or(Tz::Europe__Berlin, |loc| loc.timezone))
    }
}
