mod common;
use common::*;
use rn_std::random::*;

const RUNNER: &str = r##"
local function bits(x) return ("%016x"):format((string.unpack("<i8", string.pack("<d", x)))) end
for line in io.lines(arg[1]) do
  local f = {}
  for x in line:gmatch("%S+") do f[#f+1] = tonumber(x) end
  local seed, m, lo, hi = f[1], f[2], f[3], f[4]
  local a, b
  if m == 0 then a, b = math.randomseed(seed) else a, b = math.randomseed(seed, m) end
  local out = {a, b}
  for i = 1, 1000 do out[#out+1] = bits(math.random()) end
  for i = 1, 1000 do out[#out+1] = math.random(hi) end
  for i = 1, 1000 do out[#out+1] = math.random(lo, hi) end
  for i = 1, 100 do out[#out+1] = math.random(0) end
  for i = 1, 100 do out[#out+1] = math.random(math.mininteger, math.maxinteger) end
  print(table.concat(out, " "))
end
"##;

fn bits(f: f64) -> String {
    format!("{:016x}", f.to_bits())
}

#[test]
fn random_differential() {
    let mut r = Rng::new(77);
    let mut lines = Vec::new();
    for i in 0..120 {
        let seed: i64 = match i % 6 {
            0 => i as i64,
            1 => 42,
            2 => r.next() as i64,
            3 => -(r.below(1000) as i64),
            4 => i64::MAX - r.below(3) as i64,
            _ => r.range(-1_000_000, 1_000_000),
        };
        let m: i64 = if i % 3 == 0 {
            0
        } else {
            r.next() as i64 >> r.below(60)
        };
        let (lo, hi) = match i % 5 {
            0 => (1, 100),
            1 => (-50, 50),
            2 => (i64::MIN, i64::MAX),
            3 => (0, r.below(1 << 40) as i64),
            _ => (r.range(-1000, 0), r.range(1, 1 << 33)),
        };
        lines.push(format!("{seed} {m} {lo} {hi}"));
    }
    let data = lines.join("\n") + "\n";
    let Some(reference) = run_ref("random", false, RUNNER, &data, &[]) else {
        return;
    };
    let ours: Vec<String> = lines
        .iter()
        .map(|l| {
            let f: Vec<i64> = l.split(' ').map(|x| x.parse().unwrap()).collect();
            let (seed, m, lo, hi) = (f[0], f[1], f[2], f[3]);
            let mut g = Rand::with_seed(seed as u64, m as u64);
            let mut out = vec![seed.to_string(), m.to_string()];
            let ints = |v: RandValue| match v {
                RandValue::Int(i) => i.to_string(),
                RandValue::Float(f) => bits(f),
            };
            for _ in 0..1000 {
                out.push(ints(g.random(&[]).unwrap()).to_string());
            }
            // floats are printed as bits by the reference runner too
            for _ in 0..1000 {
                out.push(ints(g.random(&[hi]).unwrap()));
            }
            for _ in 0..1000 {
                out.push(ints(g.random(&[lo, hi]).unwrap()));
            }
            for _ in 0..100 {
                out.push(ints(g.random(&[0]).unwrap()));
            }
            for _ in 0..100 {
                out.push(ints(g.random(&[i64::MIN, i64::MAX]).unwrap()));
            }
            out.join(" ")
        })
        .collect();
    compare("random", &lines, &reference, &ours);
}

#[test]
fn random_errors() {
    let mut g = Rand::with_seed(1, 2);
    assert_eq!(
        g.random(&[3, 1]),
        Err(RandError::Arg(1, "interval is empty"))
    );
    assert_eq!(g.random(&[1, 2, 3]), Err(RandError::WrongArgs));
}
