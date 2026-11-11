//! Compare two binary chunks field by field (`cmpbin a.luac b.luac`).
use rn_compiler::testhost::TestHost;
use rn_compiler::undump;
use rn_core::protodata::ProtoData;

fn cmp(a: &ProtoData<u32>, b: &ProtoData<u32>, path: &str) {
    macro_rules! f {
        ($($n:ident),*) => {$(if a.$n != b.$n { println!("{path}: field {} differs: {:?} vs {:?}", stringify!($n), a.$n, b.$n); })*};
    }
    f!(
        num_params,
        flags,
        max_stack_size,
        line_defined,
        last_line_defined,
        line_info,
        abs_line_info
    );
    if let Some(i) = a.code.iter().zip(&b.code).position(|(x, y)| x != y) {
        println!(
            "{path}: first code diff at pc {i}: {:#010x} vs {:#010x}",
            a.code[i], b.code[i]
        );
    }
    if a.upvalues.len() != b.upvalues.len()
        || a.upvalues
            .iter()
            .zip(&b.upvalues)
            .any(|(x, y)| x.in_stack != y.in_stack || x.idx != y.idx || x.kind != y.kind)
    {
        println!(
            "{path}: upvalues differ {:?} vs {:?}",
            a.upvalues, b.upvalues
        );
    }
    if a.protos.len() != b.protos.len() {
        println!("{path}: nprotos differ");
        return;
    }
    for (i, (x, y)) in a.protos.iter().zip(&b.protos).enumerate() {
        cmp(x, y, &format!("{path}/{i}"));
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (mut h1, mut h2) = (TestHost::default(), TestHost::default());
    let a = undump(&mut h1, &std::fs::read(&args[0]).unwrap(), b"=a").unwrap();
    let b = undump(&mut h2, &std::fs::read(&args[1]).unwrap(), b"=b").unwrap();
    cmp(&a, &b, "main");
}
