use lilbits::LilBitSet;
use common::ClientId;

pub struct LilClientSet {
    set: LilBitSet,
}

impl LilClientSet {
    fn check_element_is_ok(cid: ClientId) {
        assert!(cid.0 <= ClientId(LilBitSet::largest_allowed()))
    }
    #[inline]
    pub fn new() -> LilClientSet { LilClientSet {set: LilBitSet::new() } }
    #[inline]
    pub fn largest_allowed() -> ClientId { ClientId(LilBitSet::largest_allowed()) } 
    #[inline]
    pub fn new_from_raw(raw: u64) -> Self { LilClientSet::new_from_raw(raw) }
    #[inline]
    pub fn into_raw(self) -> u64 { self.into_raw() }
    pub fn get(&self, element: ClientId) -> Option<ClientId> {
        self.set.get(element)
        .map(|x| ClientId(x))
    }
    pub fn contains(&self, element : ClientId) -> bool {
        Self::check_element_is_ok();
        self.set.contains(element.0 as u8)
    }

    #[inline]
    pub fn is_empty(&self) -> bool { self.set.is_empty() }
    pub fn universe() -> LilClientSet {
        LilClientSet { set: LilBitSet::universe() }
    }
    pub fn insert(&mut self, element: u8) -> bool {
        Self::check_element_is_ok(element);
        self.insert(element.0)
    }
    pub fn remove(&mut self, element: u8) -> bool {
        Self::check_element_is_ok(element);
        self.remove(element.0)
    }
    pub fn union(&self, other: &Self) -> Self {
        LilClientSet { set: self.set.union(other.set) }
    }
    pub fn itersection(&self, other: &Self) -> Self {
        LilClientSet { set: self.set.intesection(other.set) }
    }
    pub fn len(&self) -> usize { self.set.len() }
    pub fn symmetric_difference(&self, other: &Self) -> Self {
        LilClientSet { set: self.set.symmetric_difference(other.set) }
    }
    pub fn complement(&self) -> LilBitSet {
        LilClientSet { set: self.set.complement() }
    }
}