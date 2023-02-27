
use std::sync::Arc;

use crate::data_source::DataSource;

pub const PAGE_SIZE: usize = 4096;
pub const ADDR_MAX: usize = (1 << 38) - 1;

type VirtualAddress = usize;

struct MapEntry {
    source: Arc<dyn DataSource>,
    offset: usize,
    span: usize,
    addr: usize,
}

impl MapEntry {
    pub fn new(source: Arc<dyn DataSource>, offset: usize, span: usize, addr: usize) -> MapEntry {
        MapEntry {
            source: source.clone(),
            offset,
            span,
            addr,
        }
    }
}

/// An address space.
pub struct AddressSpace {
    name: String,
    mappings: Vec<MapEntry>, // see below for comments
}

// comments about storing mappings
// Most OS code uses doubly-linked lists to store sparse data structures like
// an address space's mappings.
// Using Rust's built-in LinkedLists is fine. See https://doc.rust-lang.org/std/collections/struct.LinkedList.html
// But if you really want to get the zen of Rust, this is a really good read, written by the original author
// of that very data structure: https://rust-unofficial.github.io/too-many-lists/

// So, feel free to come up with a different structure, either a classic Rust collection,
// from a crate (but remember it needs to be #no_std compatible), or even write your own.
// See this ticket from Riley: https://github.com/dylanmc/cs393_vm_api/issues/10

impl AddressSpace {
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            mappings: Vec::new(),
        }
    }

    /// Add a mapping from a `DataSource` into this `AddressSpace`.
    ///
    /// # Errors
    /// If the desired mapping is invalid.
    pub fn add_mapping<D: DataSource + 'static>(
        &mut self,
        source: Arc<D>,
        offset: usize,
        span: usize,
    ) -> Result<VirtualAddress, &str> {
        // basically dont map at 0, wnat to find first open space the size of span
        // if you run out of mappings to go through stick it at the end
        // unless there is no room, then say cant do it
        let mut possible_address = PAGE_SIZE;
        let mut open_space;
        let mut index:usize = 0;
        for i in &self.mappings {
            open_space = i.addr - possible_address;
            if open_space > span+2 * PAGE_SIZE {
                break;
            }
            index = index + 1;
            possible_address = i.addr + i.span;
        }
        if possible_address + span  + 2 * PAGE_SIZE < ADDR_MAX {
            possible_address = possible_address + PAGE_SIZE;
            let new_mapping = MapEntry::new(source, offset, span, possible_address);
            self.mappings.insert(index, new_mapping);
            return Ok(possible_address);
        }
        return Err("No space available");
    }

    /// Add a mapping from `DataSource` into this `AddressSpace` starting at a specific address.
    ///
    /// # Errors
    /// If there is insufficient room subsequent to `start`.
    pub fn add_mapping_at<D: DataSource+ 'static>(
        &mut self,
        source: Arc<D>,
        offset: usize,
        span: usize,
        start: VirtualAddress,
    ) -> Result<(), &str> {
        let mut current_address = self.mappings[0].addr;
        let mut index:usize = 0;
        while current_address < start {
            index = index + 1;
            current_address = self.mappings[index].addr;
        }
        if current_address - start > span {
            let previous_end = self.mappings[index-1].addr + self.mappings[index-1].span;
            if previous_end < start {
                let new_address = start + PAGE_SIZE;
                let new_mapping = MapEntry::new(source, offset, span, new_address);
                let mut temp_vec = vec![new_mapping];
                self.mappings.append(&mut temp_vec);
                return Ok(());
            } else {
                return Err("No space available");
            }
        }
        return Err("No space available");

    }

    /// Remove the mapping to `DataSource` that starts at the given address.
    ///
    /// # Errors
    /// If the mapping could not be removed.
    pub fn remove_mapping<D: DataSource>(
        &mut self,
        source: &D,
        start: VirtualAddress,
    ) -> Result<(), &str> {
        let mut index:usize = 0;
        for mappings in &self.mappings {
            if mappings.addr == start {
                self.mappings.remove(index);
                return Ok(());
            }
            index = index + 1
        }
        return Err("Not possible to remove mapping")
    }

    /// Look up the DataSource and offset within that DataSource for a
    /// VirtualAddress / AccessType in this AddressSpace
    /// 
    /// # Errors
    /// If this VirtualAddress does not have a valid mapping in &self,
    /// or if this AccessType is not permitted by the mapping
    pub fn get_source_for_addr<D: DataSource>(
        &self,
        addr: VirtualAddress,
        access_type: FlagBuilder
    ) -> Result<(&D, usize), &str> {
        let mut index:usize = 0;
        for mapping in self.mappings {
            if mapping.addr == addr { //this doesnt work to access D
                return Ok((*mapping.source, mapping.offset));
            }
        }
        return Err("Not possible to return source for address")
    }
}

/// Build flags for address space maps.
///
/// We recommend using this builder type as follows:
/// ```
/// # use reedos_address_space::FlagBuilder;
/// let flags = FlagBuilder::new()
///     .toggle_read()
///     .toggle_write();
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // clippy is wrong: bools are more readable than enums
                                         // here because these directly correspond to yes/no
                                         // hardware flags
pub struct FlagBuilder {
    // TODO: should there be some sanity checks that conflicting flags are never toggled? can we do
    // this at compile-time? (the second question is maybe hard)
    read: bool,
    write: bool,
    execute: bool,
    cow: bool,
    private: bool,
    shared: bool,
}

/// Create a constructor and toggler for a `FlagBuilder` object. Will capture attributes, including documentation
/// comments and apply them to the generated constructor.
macro_rules! flag {
    (
        $flag:ident,
        $toggle:ident
    ) => {
        #[doc=concat!("Turn on only the ", stringify!($flag), " flag.")]
        #[must_use]
        pub fn $flag() -> Self {
            Self {
                $flag: true,
                ..Self::default()
            }
        }

        #[doc=concat!("Toggle the ", stringify!($flag), " flag.")]
        #[must_use]
        pub const fn $toggle(self) -> Self {
            Self {
                $flag: !self.$flag,
                ..self
            }
        }
    };
}

impl FlagBuilder {
    /// Create a new `FlagBuilder` with all flags toggled off.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    flag!(read, toggle_read);
    flag!(write, toggle_write);
    flag!(execute, toggle_execute);
    flag!(cow, toggle_cow);
    flag!(private, toggle_private);
    flag!(shared, toggle_shared);

    #[must_use]
    /// Combine two `FlagBuilder`s by boolean or-ing each of their flags.
    ///
    /// This is, somewhat counter-intuitively, named `and`, so that the following code reads
    /// correctly:
    ///
    /// ```
    /// # use reedos_address_space::FlagBuilder;
    /// let read = FlagBuilder::read();
    /// let execute = FlagBuilder::execute();
    /// let new = read.and(execute);
    /// assert_eq!(new, FlagBuilder::new().toggle_read().toggle_execute());
    /// ```
    pub const fn and(self, other: Self) -> Self {
        let read = self.read || other.read;
        let write = self.write || other.write;
        let execute = self.execute || other.execute;
        let cow = self.cow || other.cow;
        let private = self.private || other.private;
        let shared = self.shared || other.shared;

        Self {
            read,
            write,
            execute,
            cow,
            private,
            shared,
        }
    }

    #[must_use]
    /// Turn off all flags in self that are on in other.
    ///
    /// You can think of this as `self &! other` on each field.
    ///
    /// ```
    /// # use reedos_address_space::FlagBuilder;
    /// let read_execute = FlagBuilder::read().toggle_execute();
    /// let execute = FlagBuilder::execute();
    /// let new = read_execute.but_not(execute);
    /// assert_eq!(new, FlagBuilder::new().toggle_read());
    /// ```
    pub const fn but_not(self, other: Self) -> Self {
        let read = self.read && !other.read;
        let write = self.write && !other.write;
        let execute = self.execute && !other.execute;
        let cow = self.cow && !other.cow;
        let private = self.private && !other.private;
        let shared = self.shared && !other.shared;

        Self {
            read,
            write,
            execute,
            cow,
            private,
            shared,
        }
    }
}

