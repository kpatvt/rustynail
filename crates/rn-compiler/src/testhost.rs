//! A trivial [`CompilerHost`] used by the compiler tests (and available to downstream test code).
use rn_core::host::CompilerHost;
use rn_core::status::LuaResult;
use std::collections::HashMap;

/// Interns strings into a `Vec`; handles are indices (equal bytes => equal handle).
#[derive(Default, Debug)]
pub struct TestHost {
    strings: Vec<Vec<u8>>,
    index: HashMap<Vec<u8>, u32>,
}

impl CompilerHost for TestHost {
    type Str = u32;

    fn intern(&mut self, bytes: &[u8]) -> LuaResult<u32> {
        if let Some(&i) = self.index.get(bytes) {
            return Ok(i);
        }
        let i = self.strings.len() as u32;
        self.strings.push(bytes.to_vec());
        self.index.insert(bytes.to_vec(), i);
        Ok(i)
    }

    fn bytes(&self, s: u32) -> &[u8] {
        self.strings.get(s as usize).map_or(&[], |v| v.as_slice())
    }

    fn str_id(s: u32) -> usize {
        s as usize
    }
}
