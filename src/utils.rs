use crate::{error::Error, hex_utils, node::MacaroonSession};
use macaroon::Macaroon;
use std::{
    cmp::min,
    marker::PhantomData,
    time::{SystemTime, SystemTimeError},
};

pub fn hours_since_epoch() -> Result<u64, SystemTimeError> {
    let time_since_epoch = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?;
    let hours_since_epoch = time_since_epoch.as_secs() / 3600;
    Ok(hours_since_epoch)
}

pub fn macaroon_from_hex_str(hex_str: &str) -> Result<Macaroon, Error> {
    let macaroon_byte_vec = hex_utils::to_vec(hex_str).unwrap();
    Macaroon::deserialize(macaroon_byte_vec.as_slice()).map_err(Error::Macaroon)
}

pub fn macaroon_with_session_from_hex_str(
    hex_str: &str,
) -> Result<(Macaroon, MacaroonSession), Error> {
    let macaroon = macaroon_from_hex_str(hex_str)?;
    let session = MacaroonSession::new(&macaroon)?;
    Ok((macaroon, session))
}

pub struct PagedVec<'a, T, V> {
    vec: &'a V,
    page_length: usize,
    phantom: PhantomData<&'a T>,
}

impl<'a, T, V> PagedVec<'a, T, V>
where
    V: AsRef<[T]>,
{
    pub fn new(vec: &'a V, page_length: usize) -> PagedVec<'a, T, V> {
        PagedVec {
            vec,
            page_length,
            phantom: PhantomData,
        }
    }

    pub fn page(&self, index: usize) -> Option<(usize, &'a [T])> {
        let slice = self.vec.as_ref();
        let len = slice.len();

        if index < len {
            let page_index = index % self.page_length;
            let start = index - page_index;
            let end = min(len, start + self.page_length);

            slice.get(start..end).map(|s| (page_index, s))
        } else {
            None
        }
    }
}