//! The capture-service protocol between the profiler ASI (`shipments/mercs2-profiler`) and `mercs2_profd`,
//! the service that runs `tracy-capture` for it: one request line per TCP connection, one reply line back.
//!
//! ```text
//! START <session>     →  OK <message>  |  ERR <reason>     a new capture, written to <session>.tracy
//! SAMPLES <session>   then the bytes of <session>.samples (crate::prof_samples) until the client shuts
//!                     down its sending side  →  OK <message>  |  ERR <reason>
//! STOP                →  OK <message>  |  ERR <reason>     end the capture and save its file
//! STATUS              →  OK <message>  |  ERR <reason>
//! ```
//!
//! Lines end in `\n`; a reply's text is one line. A session name is the capture's file stem, so it is
//! limited to characters that are safe in a file name on every host ([`check_session`]). A `SAMPLES`
//! stream belongs to the capture that is running, and is ended (its reply read) before that capture's
//! `STOP`.

/// The longest session name.
pub const SESSION_MAX: usize = 96;
/// The longest line either side reads.
pub const LINE_MAX: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Start { session: String },
    Samples { session: String },
    Stop,
    Status,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    Ok(String),
    Err(String),
}

/// A session name: 1..=[`SESSION_MAX`] of `A-Z a-z 0-9 . _ -`, not starting with `.` or `-`.
pub fn check_session(s: &str) -> Result<(), String> {
    if s.is_empty() || s.len() > SESSION_MAX {
        return Err(format!("a session name is 1 to {SESSION_MAX} characters, not {}", s.len()));
    }
    if let Some(c) = s.chars().find(|c| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))) {
        return Err(format!("a session name may not contain {c:?}"));
    }
    if s.starts_with(['.', '-']) {
        return Err(format!("a session name may not start with {:?}", &s[..1]));
    }
    Ok(())
}

/// The session name for a capture started at local time `y-mo-d h:mi:s`: `YYYYMMDD-HHMMSS`.
pub fn session_at(y: u16, mo: u16, d: u16, h: u16, mi: u16, s: u16) -> String {
    format!("{y:04}{mo:02}{d:02}-{h:02}{mi:02}{s:02}")
}

fn one_line(text: &str) -> Result<&str, String> {
    let t = text.strip_suffix('\n').unwrap_or(text);
    let t = t.strip_suffix('\r').unwrap_or(t);
    if t.len() > LINE_MAX {
        return Err(format!("a line is at most {LINE_MAX} bytes, not {}", t.len()));
    }
    if t.contains(['\n', '\r']) {
        return Err("a line may not contain a line break".into());
    }
    Ok(t)
}

impl Request {
    pub fn parse(line: &str) -> Result<Self, String> {
        let t = one_line(line)?;
        let (verb, rest) = t.split_once(' ').unwrap_or((t, ""));
        match (verb, rest) {
            ("START", session) => {
                check_session(session)?;
                Ok(Request::Start { session: session.to_owned() })
            }
            ("SAMPLES", session) => {
                check_session(session)?;
                Ok(Request::Samples { session: session.to_owned() })
            }
            ("STOP", "") => Ok(Request::Stop),
            ("STATUS", "") => Ok(Request::Status),
            ("STOP" | "STATUS", _) => Err(format!("{verb} takes no argument")),
            _ => Err(format!("unknown request {verb:?}")),
        }
    }

    /// The request as sent, with its `\n`.
    pub fn line(&self) -> String {
        match self {
            Request::Start { session } => format!("START {session}\n"),
            Request::Samples { session } => format!("SAMPLES {session}\n"),
            Request::Stop => "STOP\n".into(),
            Request::Status => "STATUS\n".into(),
        }
    }
}

impl Reply {
    pub fn parse(line: &str) -> Result<Self, String> {
        let t = one_line(line)?;
        match t.split_once(' ') {
            Some(("OK", m)) => Ok(Reply::Ok(m.to_owned())),
            Some(("ERR", m)) => Ok(Reply::Err(m.to_owned())),
            _ if t == "OK" => Ok(Reply::Ok(String::new())),
            _ => Err(format!("not a reply: {t:?}")),
        }
    }

    /// The reply as sent, with its `\n`; line breaks in the text become spaces.
    pub fn line(&self) -> String {
        let (tag, text) = match self {
            Reply::Ok(m) => ("OK", m),
            Reply::Err(m) => ("ERR", m),
        };
        let text: String = text.chars().map(|c| if matches!(c, '\n' | '\r') { ' ' } else { c }).collect();
        if text.is_empty() { format!("{tag}\n") } else { format!("{tag} {text}\n") }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_round_trip() {
        for r in [
            Request::Start { session: "20261022-031650".into() },
            Request::Samples { session: "20261022-031650".into() },
            Request::Stop,
            Request::Status,
        ] {
            assert_eq!(Request::parse(&r.line()).unwrap(), r);
        }
        assert_eq!(Request::parse("STOP\r\n").unwrap(), Request::Stop);
    }

    #[test]
    fn bad_requests_say_why() {
        assert!(Request::parse("START").unwrap_err().contains("1 to 96"));
        assert!(Request::parse("SAMPLES a/b").unwrap_err().contains("'/'"));
        assert!(Request::parse("START ../etc/passwd").unwrap_err().contains("'/'"));
        assert!(Request::parse("START .hidden").unwrap_err().contains("may not start"));
        assert!(Request::parse("STOP now").unwrap_err().contains("takes no argument"));
        assert!(Request::parse("start x").unwrap_err().contains("unknown request"));
        assert!(Request::parse("START a\nSTOP").unwrap_err().contains("line break"));
    }

    #[test]
    fn replies_round_trip_on_one_line() {
        for r in [Reply::Ok("recording a.tracy".into()), Reply::Err("no capture".into()), Reply::Ok(String::new())] {
            assert_eq!(Reply::parse(&r.line()).unwrap(), r);
        }
        assert_eq!(Reply::Err("two\nlines".into()).line(), "ERR two lines\n");
        assert!(Reply::parse("MAYBE").is_err());
    }

    #[test]
    fn sessions_are_timestamps() {
        assert_eq!(session_at(2026, 10, 22, 3, 16, 50), "20261022-031650");
        check_session(&session_at(2026, 1, 2, 3, 4, 5)).unwrap();
        assert!(check_session(&"x".repeat(SESSION_MAX + 1)).is_err());
    }
}
