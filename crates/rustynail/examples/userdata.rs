//! A Rust type exposed to Lua as userdata with methods, metamethods and a constructor.
//!
//! `cargo run -p rustynail --example userdata`
use rustynail::{Error, Lua, Stack};

/// A bank account owned by Lua.
#[derive(Debug)]
struct Account {
    owner: String,
    balance: i64,
}

fn amount(s: &mut dyn Stack, i: i32) -> rustynail::Result<i64> {
    s.to_integer(i)
        .filter(|&a| a >= 0)
        .ok_or_else(|| Error::runtime("amount must be a non-negative integer"))
}

fn register(lua: &mut Lua) -> rustynail::Result<()> {
    // methods live in the metatable of the Rust type (shared by all `Account` userdata)
    lua.push_userdata_metatable::<Account>()?;
    lua.new_table()?; // the methods table, used as __index
    lua.push_function(|s| {
        let a = amount(s, 2)?;
        s.with_userdata_mut(1, |acc: &mut Account| acc.balance += a)?;
        s.push_value(1)?; // return self for chaining
        Ok(1)
    })?;
    lua.set_field(-2, "deposit")?;
    lua.push_function(|s| {
        let a = amount(s, 2)?;
        let ok = s.with_userdata_mut(1, |acc: &mut Account| {
            if acc.balance >= a {
                acc.balance -= a;
                true
            } else {
                false
            }
        })?;
        if !ok {
            return Err(Error::runtime("insufficient funds"));
        }
        s.push_value(1)?;
        Ok(1)
    })?;
    lua.set_field(-2, "withdraw")?;
    lua.push_function(|s| {
        let b = s.with_userdata(1, |acc: &Account| acc.balance)?;
        s.push_integer(b)?;
        Ok(1)
    })?;
    lua.set_field(-2, "balance")?;
    lua.set_field(-2, "__index")?;
    lua.push_function(|s| {
        let text = s.with_userdata(1, |acc: &Account| {
            format!("Account({}, {})", acc.owner, acc.balance)
        })?;
        s.push_str(&text)?;
        Ok(1)
    })?;
    lua.set_field(-2, "__tostring")?;
    lua.pop(1)?;

    // constructor: Account.new(owner)
    lua.new_table()?;
    lua.push_function(|s| {
        let owner = s
            .to_string_lossy(1)
            .ok_or_else(|| Error::runtime("owner name expected"))?;
        s.push_userdata(Account { owner, balance: 0 })?;
        Ok(1)
    })?;
    lua.set_field(-2, "new")?;
    lua.set_global("Account")
}

fn run() -> rustynail::Result<(String, i64)> {
    let mut lua = Lua::new()?;
    register(&mut lua)?;
    lua.do_string(
        r#"
        acct = Account.new("ada"):deposit(100):withdraw(30)
        print(tostring(acct))
        print(pcall(acct.withdraw, acct, 1000))
        "#,
    )?;
    // read the object back from Rust
    lua.get_global("acct")?;
    let (owner, balance) = lua.with_userdata(-1, |a: &Account| (a.owner.clone(), a.balance))?;
    lua.pop(1)?;
    Ok((owner, balance))
}

fn main() {
    match run() {
        Ok((o, b)) => println!("{o} has {b}"),
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}

#[test]
fn example_runs() {
    assert_eq!(run().unwrap(), ("ada".to_string(), 70));
}
