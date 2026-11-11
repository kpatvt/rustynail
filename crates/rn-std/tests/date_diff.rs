mod common;
use common::*;
use rn_std::strtime::*;

const RUNNER: &str = r##"
local function hex(s) return (s:gsub(".", function(c) return string.format("%02x", c:byte()) end)) end
local function unhex(s) return (s:gsub("%x%x", function(h) return string.char(tonumber(h, 16)) end)) end
local function err(m) return "E" .. hex((m:gsub("to '[^']*'", "to '?'"))) end
local function tfmt(t)
  return table.concat({t.year, t.month, t.day, t.hour, t.min, t.sec, t.yday, t.wday, tostring(t.isdst)}, ",")
end
for line in io.lines(arg[1]) do
  local f = {}
  for x in (line .. "\t"):gmatch("(.-)\t") do f[#f+1] = x end
  local out
  if f[1] == "date" then
    local r = table.pack(pcall(os.date, unhex(f[2]), math.tointeger(f[3])))
    if r[1] then
      if type(r[2]) == "table" then out = "T" .. tfmt(r[2]) else out = "s" .. hex(r[2]) end
    else out = err(r[2]) end
  elseif f[1] == "time" then
    local t = {year = tonumber(f[2]), month = tonumber(f[3]), day = tonumber(f[4]), hour = tonumber(f[5]), min = tonumber(f[6]), sec = tonumber(f[7])}
    if f[8] == "1" then t.isdst = true elseif f[8] == "0" then t.isdst = false end
    local r = table.pack(pcall(os.time, t))
    if r[1] then out = "i" .. r[2] .. " " .. tfmt(t) else out = err(r[2]) end
  else
    out = tostring(os.difftime(math.tointeger(f[2]), math.tointeger(f[3])))
  end
  print(out)
end
"##;

const SPECS: &str = "aAbBcCdDeFgGhHIjmMnprRStTuUVwWxXyYzZ%";
const TWO: &[&str] = &[
    "Ec", "EC", "Ex", "EX", "Ey", "EY", "Od", "Oe", "OH", "OI", "Om", "OM", "OS", "Ou", "OU", "OV",
    "Ow", "OW", "Oy",
];

fn fmt_tm(f: &Fields) -> String {
    format!(
        "{},{},{},{},{},{},{},{},{}",
        f.year,
        f.month,
        f.day,
        f.hour,
        f.min,
        f.sec,
        f.yday,
        f.wday,
        match f.isdst {
            Some(b) => b.to_string(),
            None => "nil".into(),
        }
    )
}

fn ours_date(format: &[u8], t: i64) -> String {
    match date(format, t) {
        Ok(DateOut::Table(tm)) => format!("T{}", fmt_tm(&tm.fields())),
        Ok(DateOut::Text(b)) => format!("s{}", hex(&b)),
        Err(DateError::Msg(m)) => format!("E{}", hex(&m)),
        Err(DateError::Arg(n, m)) => {
            let mut full = format!("bad argument #{n} to '?' (").into_bytes();
            full.extend_from_slice(&m);
            full.push(b')');
            format!("E{}", hex(&full))
        }
    }
}

fn ours_time(f: &[i64], isdst: &str) -> String {
    let names = ["year", "month", "day", "hour", "min", "sec"];
    let deltas = [1900i64, 1, 0, 0, 0, 0];
    let mut c = [0i32; 6];
    for i in 0..6 {
        match field_to_c(f[i], deltas[i]) {
            Some(v) => c[i] = v,
            None => {
                return format!(
                    "E{}",
                    hex(format!("field '{}' is out-of-bound", names[i]).as_bytes())
                )
            }
        }
    }
    let d = match isdst {
        "1" => 1,
        "0" => 0,
        _ => -1,
    };
    let (t, tm) = time_from_tm(Tm::from_c_fields(c[0], c[1], c[2], c[3], c[4], c[5], d));
    if !time_ok(t) {
        return format!(
            "E{}",
            hex(b"time result cannot be represented in this installation")
        );
    }
    format!("i{} {}", t, fmt_tm(&tm.fields()))
}

fn run_zone(tz: &str) {
    if !std::path::Path::new("/usr/share/zoneinfo")
        .join(tz)
        .exists()
        && tz != "UTC"
    {
        eprintln!("SKIP zone {tz}");
        return;
    }
    std::env::set_var("TZ", tz);
    tzset();
    let mut r = Rng::new(0xDA7E ^ tz.len() as u64);
    let mut lines: Vec<String> = Vec::new();
    let mut ours: Vec<String> = Vec::new();
    let mut stamps: Vec<i64> = Vec::new();
    for i in 0..1200 {
        stamps.push(match i % 6 {
            0 => r.range(0, 2_000_000_000),
            1 => r.range(-2_000_000_000, 0),
            2 => r.range(1_600_000_000, 1_700_000_000),
            3 => r.range(-10_000_000_000, 10_000_000_000),
            4 => 1_678_604_400 + r.range(-7200, 7200), // around a US DST start
            _ => r.range(0, 4_000_000_000),
        });
    }
    stamps.extend_from_slice(&[0, -1, 1, 86399, 951782400, 1_000_000_000_000]);
    for (k, &t) in stamps.iter().enumerate() {
        let mut fmts: Vec<String> = SPECS.chars().map(|c| format!("%{c}")).collect();
        fmts.extend(TWO.iter().map(|s| format!("%{s}")));
        fmts.push("*t".into());
        fmts.push("!*t".into());
        fmts.push("%Y-%m-%d %H:%M:%S %Z %z".into());
        fmts.push("!%c|%x|%X|%p|%a %b".into());
        if k % 50 == 0 {
            // error cases
            fmts.extend(
                [
                    "%", "%E", "%Ez", "%Q", "abc%y%", "%O", "%Oa", "!%Ex%k", "%5d", "%_d",
                ]
                .iter()
                .map(|s| s.to_string()),
            );
        }
        for f in fmts {
            lines.push(format!("date\t{}\t{}", hex(f.as_bytes()), t));
            ours.push(ours_date(f.as_bytes(), t));
        }
    }
    for _ in 0..3000 {
        let big = r.chance(1, 20);
        let f: Vec<i64> = vec![
            if big {
                r.range(-3_000_000_000, 3_000_000_000)
            } else {
                r.range(1900, 2100)
            },
            if big {
                r.range(-100, 100)
            } else {
                r.range(-14, 26)
            },
            if big {
                r.range(-1000, 1000)
            } else {
                r.range(-40, 70)
            },
            r.range(-30, 60),
            r.range(-100, 100),
            r.range(-100, 100),
        ];
        let isdst = *r.pick(&["n", "0", "1", "n"]);
        lines.push(format!(
            "time\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            f[0], f[1], f[2], f[3], f[4], f[5], isdst
        ));
        ours.push(ours_time(&f, isdst));
    }
    let data = lines.join("\n") + "\n";
    let Some(reference) = run_ref(
        &format!("date-{}", tz.replace('/', "_")),
        false,
        RUNNER,
        &data,
        &[("TZ", tz)],
    ) else {
        return;
    };
    compare(&format!("date[{tz}]"), &lines, &reference, &ours);
}

#[test]
fn date_differential() {
    // sequential: TZ is process-global
    run_zone("UTC");
    run_zone("America/New_York");
    run_zone("Europe/London");
    run_zone("Australia/Lord_Howe");
}

#[test]
fn check_option_basics() {
    assert_eq!(check_option(b"Y rest"), Ok(1));
    assert_eq!(check_option(b"Ey"), Ok(2));
    assert!(check_option(b"").is_err());
    assert!(check_option(b"Q").is_err());
}
