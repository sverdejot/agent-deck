//! Serial protocol with the Mac (115200 baud, one line per message, TAB-separated fields).
//! The full specification is in `docs/protocol.md`.

/// No message for this long and the link counts as lost.
pub const LINK_TIMEOUT_MS: u32 = 7000;
/// Most repos shown in the Usage chart (the Mac folds the rest into "otros").
pub const MAX_SERIES: usize = 4;
/// 15-minute buckets in a day.
pub const MAX_BUCKETS: usize = 96;

/// Sent until the first snapshot arrives.
pub const HELLO: &str = "H\n";

/// Asks the Mac to bring a session to the front.
pub fn open_msg(id: &str) -> String {
    format!("O\t{id}\n")
}

/// Asks the Mac for the summary of a session.
pub fn summary_msg(id: &str) -> String {
    format!("R\t{id}\n")
}

#[derive(Clone, PartialEq, Eq, Default, Debug)]
pub struct Session {
    pub id: String,
    /// `busy`, `wait`, `ready` or `idle`
    pub state: String,
    pub title: String,
    /// repository name (or short path)
    pub place: String,
    /// "hace 3 h"; empty while working
    pub age: String,
    /// context window used, 0-100; -1 unknown
    pub ctx: i32,
}

impl Session {
    pub fn busy(&self) -> bool {
        self.state == "busy"
    }

    pub fn waiting(&self) -> bool {
        self.state == "wait"
    }

    pub fn state_label(&self) -> &'static str {
        match self.state.as_str() {
            "busy" => "Trabajando",
            "wait" => "Te necesita",
            "ready" => "Lista",
            _ => "Inactiva",
        }
    }
}

#[derive(Default, Debug)]
pub struct Summary {
    pub id: String,
    pub loaded: bool,
    pub meta: String,
    pub you: String,
    pub claude: String,
    pub tool: String,
}

#[derive(Debug)]
pub struct Limits {
    /// % of the 5-hour window used; -1 until the first reading
    pub five: i32,
    pub five_reset: String,
    /// % of the 5-hour window already gone
    pub five_elapsed: i32,
    pub week: i32,
    pub week_reset: String,
    pub week_elapsed: i32,
    pub projection: String,
    pub updated: String,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            five: -1,
            five_reset: String::new(),
            five_elapsed: 0,
            week: -1,
            week_reset: String::new(),
            week_elapsed: 0,
            projection: String::new(),
            updated: String::new(),
        }
    }
}

#[derive(Clone, Default, Debug)]
pub struct Usage {
    /// first 15-minute bucket shown (counted from midnight)
    pub start: usize,
    pub n: usize,
    pub total: String,
    pub subtitle: String,
    /// (repo, total, thousands of tokens per bucket)
    pub series: Vec<(String, String, Vec<u32>)>,
    /// (day label, thousands of tokens), oldest first
    pub days: Vec<(String, u32)>,
    pub today: usize,
}

/// What a batch of input changed, to decide what to redraw.
#[derive(Default, Clone, Copy, Debug)]
pub struct Changed {
    pub sessions: bool,
    pub summary: bool,
    pub limits: bool,
    pub usage: bool,
}

/// Everything the Mac has told us.
#[derive(Default, Debug)]
pub struct State {
    pub sessions: Vec<Session>,
    incoming: Vec<Session>,
    /// summary view: open while `Some`
    pub summary: Option<Summary>,
    pub limits: Limits,
    pub usage: Option<Usage>,
    usage_in: Usage,
    last_msg: u32,
    pub ever_linked: bool,
    line: Vec<u8>,
}

impl State {
    pub fn linked(&self, now: u32) -> bool {
        self.ever_linked && now.wrapping_sub(self.last_msg) < LINK_TIMEOUT_MS
    }

    /// Feeds raw serial bytes; complete lines are handled as they appear.
    pub fn feed(&mut self, bytes: &[u8], now: u32) -> Changed {
        let mut ch = Changed::default();
        for &b in bytes {
            match b {
                b'\r' => {}
                b'\n' => {
                    let line = std::mem::take(&mut self.line);
                    if !line.is_empty() {
                        self.handle(&String::from_utf8_lossy(&line), now, &mut ch);
                    }
                }
                _ if self.line.len() < 2048 => self.line.push(b),
                _ => {}
            }
        }
        ch
    }

    fn handle(&mut self, line: &str, now: u32, ch: &mut Changed) {
        self.last_msg = now;
        let f: Vec<&str> = line.split('\t').collect();
        let get = |i: usize| f.get(i).copied().unwrap_or("");
        match f[0] {
            "B" => self.incoming.clear(),
            "S" if f.len() >= 6 => self.incoming.push(Session {
                id: get(1).into(),
                state: get(3).into(),
                title: get(4).into(),
                place: get(5).into(),
                age: get(6).into(),
                ctx: get(7).parse().unwrap_or(-1),
            }),
            "E" => {
                if self.incoming != self.sessions {
                    self.sessions = std::mem::take(&mut self.incoming);
                    ch.sessions = true;
                }
                if !self.ever_linked {
                    ch.sessions = true;
                }
                self.ever_linked = true;
            }
            "Y" if f.len() >= 6 => {
                if let Some(s) = self.summary.as_mut().filter(|s| s.id == get(1)) {
                    s.meta = get(2).into();
                    s.you = get(3).into();
                    s.claude = get(4).into();
                    s.tool = get(5).into();
                    s.loaded = true;
                    ch.summary = true;
                }
            }
            "L" if f.len() >= 9 => {
                self.limits = Limits {
                    five: get(1).parse().unwrap_or(-1),
                    five_reset: get(2).into(),
                    five_elapsed: get(3).parse().unwrap_or(0),
                    week: get(4).parse().unwrap_or(-1),
                    week_reset: get(5).into(),
                    week_elapsed: get(6).parse().unwrap_or(0),
                    projection: get(7).into(),
                    updated: get(8).into(),
                };
                ch.limits = true;
            }
            "C" if f.len() >= 2 => self.usage_line(&f, ch),
            _ => {} // 'P' keepalive, or boot noise
        }
    }

    fn usage_line(&mut self, f: &[&str], ch: &mut Changed) {
        let get = |i: usize| f.get(i).copied().unwrap_or("");
        let csv = |s: &str| s.split(',').map(|v| v.parse().unwrap_or(0)).collect::<Vec<u32>>();
        let u = &mut self.usage_in;
        match f[1] {
            "B" => *u = Usage::default(),
            "H" if f.len() >= 6 => {
                u.start = get(2).parse().unwrap_or(0);
                u.n = get(3).parse::<usize>().unwrap_or(0).min(MAX_BUCKETS);
                u.total = get(4).into();
                u.subtitle = get(5).into();
            }
            "S" if f.len() >= 5 && u.series.len() < MAX_SERIES => {
                u.series.push((get(2).into(), get(3).into(), csv(get(4))));
            }
            "D" if f.len() >= 5 => {
                let values = csv(get(3));
                u.days = get(2).split(',').zip(values).take(7).map(|(l, v)| (l.to_string(), v)).collect();
                u.today = get(4).parse().unwrap_or(6);
            }
            "E" => {
                self.usage = Some(u.clone());
                ch.usage = true;
            }
            _ => {}
        }
    }
}
