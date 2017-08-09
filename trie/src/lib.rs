extern crate bigint;
extern crate rlp;
extern crate sha3;
extern crate blockchain;
#[cfg(test)] extern crate hexutil;

pub mod merkle;
mod cache;
mod database;

use bigint::H256;
use rlp::Rlp;
use sha3::{Digest, Keccak256};
use std::collections::HashMap;
use merkle::{MerkleValue, MerkleNode};
use merkle::nibble::{self, NibbleVec, NibbleSlice, Nibble};
use std::ops::{Deref, DerefMut};
use std::borrow::Borrow;
use std::clone::Clone;
use blockchain::Hashable;

use self::cache::Cache;
use self::database::{Database, Change, ChangeSet};

macro_rules! empty_nodes {
    () => (
        [MerkleValue::Empty, MerkleValue::Empty,
         MerkleValue::Empty, MerkleValue::Empty,
         MerkleValue::Empty, MerkleValue::Empty,
         MerkleValue::Empty, MerkleValue::Empty,
         MerkleValue::Empty, MerkleValue::Empty,
         MerkleValue::Empty, MerkleValue::Empty,
         MerkleValue::Empty, MerkleValue::Empty,
         MerkleValue::Empty, MerkleValue::Empty]
    )
}

pub fn empty_trie_hash() -> H256 {
    H256::from("0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421")
}

pub type MemoryTrie = Trie<HashMap<H256, Vec<u8>>>;

#[derive(Clone, Debug)]
pub struct Trie<D: Database> {
    database: D,
    root: H256,
}

impl<D: Database> Hashable<H256> for Trie<D> {
    fn hash(&self) -> H256 {
        self.root()
    }
}

impl<D: Database> Trie<D> {
    pub fn root(&self) -> H256 {
        self.root
    }

    pub fn set_root(&mut self, root: H256) {
        self.root = root;
    }

    pub fn is_empty(&self) -> bool {
        self.root() == empty_trie_hash()
    }

    pub fn empty(database: D) -> Self {
        Self {
            database,
            root: empty_trie_hash()
        }
    }

    fn copy_nodes<'a, 'b>(old_nodes: &'a [MerkleValue<'b>]) -> [MerkleValue<'b>; 16] {
        debug_assert!(old_nodes.len() == 16);
        let mut nodes = empty_nodes!();
        for i in 0..16 {
            nodes[i] = old_nodes[i].clone();
        }
        nodes
    }

    fn build_value<'a, 'b>(database: &'a mut Change<'b, D>, node: MerkleNode<'b>) -> MerkleValue<'b> {
        if node.inlinable() {
            MerkleValue::Full(Box::new(node))
        } else {
            let subnode = rlp::encode(&node).to_vec();
            let hash = H256::from(Keccak256::digest(&subnode).as_slice());
            database.set(hash, subnode);
            MerkleValue::Hash(hash)
        }
    }

    fn build_submap<'a, 'b: 'a, T: Iterator<Item=(&'a NibbleVec, &'a &'b [u8])>>(
        common_len: usize, map: T
    ) -> HashMap<NibbleVec, &'b [u8]> {
        let mut submap = HashMap::new();
        for (key, value) in map {
            submap.insert(key.split_at(common_len).1.into(), value.clone());
        }
        submap
    }

    fn build_node<'a, 'b>(database: &'a mut Change<'b, D>, map: &HashMap<NibbleVec, &'b [u8]>) -> MerkleNode<'b> {
        if map.len() == 0 {
            panic!();
        }

        if map.len() == 1 {
            let key = map.keys().next().unwrap();
            return MerkleNode::Leaf(key.clone(), map.get(key).unwrap().clone());
        }

        debug_assert!(map.len() > 1);

        let common: NibbleSlice = nibble::common_all(map.keys().map(|v| v.as_ref()));

        if common.len() >= 1 {
            let submap = Self::build_submap(common.len(), map.iter());
            debug_assert!(submap.len() > 0);
            let node = Self::build_node(database, &submap);
            let value = Self::build_value(database, node);
            return MerkleNode::Extension(common.into(), value);
        }

        let mut nodes = empty_nodes!();

        for i in 0..16 {
            let nibble_index: Nibble = i.into();

            let submap = Self::build_submap(1, map.iter().filter(|&(key, value)| {
                key.len() > 0 && key[0] == nibble_index
            }));
            let value = if submap.len() == 0 {
                MerkleValue::Empty
            } else {
                let node = Self::build_node(database, &submap);
                Self::build_value(database, node)
            };
            nodes[i] = value;
        }

        let additional = map.iter()
            .filter(|&(key, value)| key.len() == 0).next()
            .map(|(key, value)| value.clone());

        return MerkleNode::Branch(nodes, additional);
    }

    pub fn build(mut database: D, map: &HashMap<Vec<u8>, Vec<u8>>) -> Self {
        if map.len() == 0 {
            return Self::empty(database);
        }

        let mut node_map = HashMap::new();
        for (key, value) in map {
            node_map.insert(nibble::from_key(key.as_ref()), value.as_ref());
        }

        let (changeset, root_rlp) = {
            let mut change = Change::new(&database);
            let node = Self::build_node(&mut change, &node_map);
            (ChangeSet::from(change), rlp::encode(&node).to_vec())
        };
        let hash = H256::from(Keccak256::digest(&root_rlp).as_slice());
        changeset.drain(&mut database, true);
        database.set(hash, root_rlp);

        Trie {
            database,
            root: hash
        }
    }
    
    fn get_by_value<'a, 'b>(database: &'a mut Change<D>, cache: &'a Cache, nibble: NibbleVec, value: MerkleValue<'a>) -> Option<&'a [u8]> {
        match value {
            MerkleValue::Empty => None,
            MerkleValue::Full(sub_node) => {
                let sub_node: &MerkleNode<'a> = sub_node.borrow();
                let sub_node: MerkleNode<'a> = (*sub_node).clone();
                Self::get_by_node(database, cache, nibble, sub_node)
            },
            MerkleValue::Hash(h) => {
                let dbv = match database.get(h) {
                    Some(val) => val,
                    None => return None,
                };
                let node = cache.insert(h, dbv);
                Self::get_by_node(database, cache, nibble, node)
            },
        }
    }

    fn get_by_node<'a, 'b>(database: &'a mut Change<D>, cache: &'a Cache, nibble: NibbleVec, node: MerkleNode<'a>) -> Option<&'a [u8]> {
        match node {
            MerkleNode::Leaf(node_nibble, node_value) => {
                if node_nibble == nibble {
                    Some(node_value.into())
                } else {
                    None
                }
            },
            MerkleNode::Extension(node_nibble, node_value) => {
                if nibble.starts_with(&node_nibble) {
                    Self::get_by_value(database, cache,
                                       nibble.split_at(node_nibble.len()).1.into(),
                                       node_value.clone())
                } else {
                    None
                }
            },
            MerkleNode::Branch(nodes, additional) => {
                if nibble.len() == 0 {
                    additional.clone().map(|v| v.into())
                } else {
                    let nibble_index: usize = nibble[0].into();
                    let node = &nodes[nibble_index];
                    Self::get_by_value(database, cache,
                                       nibble.split_at(1).1.into(), node.clone())
                }
            },
        }
    }

    pub fn get<'a, 'b>(&'a self, key: &'b [u8]) -> Option<Vec<u8>> {
        if self.is_empty() {
            return None;
        }

        let nibble = nibble::from_key(key);
        let dbv = match self.database.get(self.root) {
            Some(val) => val,
            None => return None,
        };
        let node = MerkleNode::decode(&Rlp::new(&dbv));
        let mut change = Change::new(&self.database);
        let cache = Cache::new();
        let ret = Self::get_by_node(&mut change, &cache, nibble, node).map(|v| v.into());
        debug_assert!(change.inserted().len() == 0 && change.freed().len() == 0);
        ret
    }

    fn insert_by_value<'a, 'b: 'a>(
        database: &mut Change<'a, D>, cache: &'a Cache,
        nibble: NibbleVec, merkle: MerkleValue<'a>, value: &'a [u8]
    ) -> MerkleValue<'a> {
        match merkle {
            MerkleValue::Empty => {
                let mut node_map = HashMap::new();
                node_map.insert(nibble, value);

                let new_node = Self::build_node(database, &node_map);
                Self::build_value(database, new_node)
            },
            MerkleValue::Full(ref sub_node) => {
                let sub_node: &MerkleNode<'a> = sub_node.borrow();
                let sub_node: MerkleNode<'a> = (*sub_node).clone();

                let new_node = Self::insert_by_node(database, cache, nibble, sub_node, value);
                Self::build_value(database, new_node)
            },
            MerkleValue::Hash(h) => {
                let dbv = match database.get(h) {
                    Some(val) => val,
                    None => panic!(),
                };
                let node = cache.insert(h, dbv);
                let new_node = Self::insert_by_node(database, cache, nibble, node, value);
                Self::build_value(database, new_node)
            }
        }
    }

    fn insert_by_node<'a, 'b: 'a>(
        database: &mut Change<'a, D>, cache: &'a Cache,
        nibble: NibbleVec, node: MerkleNode<'a>, value: &'a [u8]
    ) -> MerkleNode<'a> {
        match node {
            MerkleNode::Leaf(ref node_nibble, ref node_value) => {
                let mut node_map = HashMap::new();
                node_map.insert(node_nibble.clone(), node_value.clone());
                node_map.insert(nibble, value);

                Self::build_node(database, &node_map)
            },
            MerkleNode::Extension(ref node_nibble, ref node_value) => {
                if nibble.starts_with(node_nibble) {
                    MerkleNode::Extension(
                        node_nibble.clone(),
                        Self::insert_by_value(
                            database, cache, nibble.split_at(node_nibble.len()).1.into(),
                            node_value.clone(), value))
                } else {
                    let common = nibble::common(&nibble, &node_nibble);
                    let rest_len = node_nibble.len() - common.len() - 1;
                    debug_assert!(node_nibble.len() - common.len() > 0);
                    debug_assert!(nibble.len() - common.len() > 0);
                    let rest_at: usize = node_nibble[common.len()].into();
                    let insert_at: usize = nibble[common.len()].into();

                    let rest = if rest_len > 0 {
                        let new_node = MerkleNode::Extension(
                            node_nibble.split_at(common.len() + 1).1.into(),
                            node_value.clone());
                        Self::build_value(database, new_node)
                    } else /* if rest_len == 0 */ {
                        node_value.clone()
                    };

                    let branched_node = {
                        let mut nodes = empty_nodes!();
                        nodes[rest_at] = rest;
                        nodes[insert_at] = Self::insert_by_value(
                            database, cache, nibble.split_at(common.len() + 1).1.into(),
                            MerkleValue::Empty, value);
                        MerkleNode::Branch(nodes, None)
                    };
                    let branched = Self::build_value(database, branched_node.clone());

                    if common.len() >= 1 {
                        MerkleNode::Extension(common.into(), branched)
                    } else /* if common.len() == 0 */ {
                        branched_node
                    }
                }
            },
            MerkleNode::Branch(ref node_nodes, ref node_additional) => {
                let mut nodes = Self::copy_nodes(node_nodes);
                if nibble.len() == 0 {
                    MerkleNode::Branch(nodes, Some(value))
                } else {
                    let nibble_index: usize = nibble[0].into();
                    let prev = nodes[nibble_index].clone();
                    nodes[nibble_index] = Self::insert_by_value(
                        database, cache, nibble.split_at(1).1.into(), prev, value);
                    MerkleNode::Branch(nodes, node_additional.clone())
                }
            },
        }
    }

    pub fn insert(&mut self, key: Vec<u8>, value: Vec<u8>) {
        let key: &[u8] = key.as_ref();
        let value: &[u8] = value.as_ref();

        let (changeset, root_rlp) = {
            let cache = Cache::new();
            let mut change = Change::new(&self.database);
            let node = if self.is_empty() {
                let mut node_map = HashMap::new();
                node_map.insert(nibble::from_key(key), value.clone());

                Self::build_node(&mut change, &node_map)
            } else {
                let nibble = nibble::from_key(key);
                let dbv = match self.database.get(self.root) {
                    Some(val) => val,
                    None => panic!(),
                };
                let node = cache.insert(self.root, dbv);
                Self::insert_by_node(&mut change, &cache, nibble, node, value)
            };
            (ChangeSet::from(change), rlp::encode(&node).to_vec())
        };
        let hash = H256::from(Keccak256::digest(&root_rlp).as_slice());
        changeset.drain(&mut self.database, true);
        self.database.set(hash, root_rlp);

        self.root = hash;
    }

    fn remove_by_value<'a, 'b: 'a>(
        database: &mut Change<'a, D>, cache: &'a Cache,
        nibble: NibbleVec, merkle: MerkleValue<'a>
    ) -> MerkleValue<'a> {
        match merkle {
            MerkleValue::Empty => {
                MerkleValue::Empty
            },
            MerkleValue::Full(ref sub_node) => {
                let sub_node: &MerkleNode<'a> = sub_node.borrow();
                let sub_node: MerkleNode<'a> = (*sub_node).clone();

                let new_node = Self::remove_by_node(database, cache, nibble, sub_node);
                if new_node.is_none() {
                    MerkleValue::Empty
                } else {
                    let new_node = new_node.unwrap();
                    Self::build_value(database, new_node)
                }
            },
            MerkleValue::Hash(h) => {
                let dbv = match database.get(h) {
                    Some(val) => val,
                    None => panic!(),
                };
                let node = cache.insert(h, dbv);
                let new_node = Self::remove_by_node(database, cache, nibble, node);
                if new_node.is_none() {
                    MerkleValue::Empty
                } else {
                    let new_node = new_node.unwrap();
                    Self::build_value(database, new_node)
                }
            },
        }
    }

    fn collapse<'a, 'b: 'a>(
        database: &mut Change<'a, D>, cache: &'a Cache, node: MerkleNode<'a>
    ) -> MerkleNode<'a> {
        fn find_subnode<'a: 'b, 'b, D: Database>(
            database: &mut Change<'a, D>, cache: &'a Cache, value: MerkleValue<'b>
        ) -> MerkleNode<'b> {
            match value {
                MerkleValue::Empty => panic!(),
                MerkleValue::Hash(h) => {
                    let dbv = match database.get(h) {
                        Some(val) => val,
                        None => panic!(),
                    };
                    cache.insert(h, dbv)
                },
                MerkleValue::Full(f) => {
                    let t: &MerkleNode = &f;
                    t.clone()
                },
            }
        }

        match node {
            MerkleNode::Leaf(_, _) => panic!(), // Leaf does not collapse.
            MerkleNode::Extension(node_nibble, node_value) => {
                let subnode = find_subnode(database, cache, node_value.clone());

                match subnode {
                    MerkleNode::Leaf(mut sub_nibble, sub_value) => {
                        let mut new_sub_nibble = node_nibble.clone();
                        new_sub_nibble.append(&mut sub_nibble);
                        MerkleNode::Leaf(new_sub_nibble, sub_value)
                    },
                    MerkleNode::Extension(mut sub_nibble, sub_value) => {
                        let mut new_sub_nibble = node_nibble.clone();
                        new_sub_nibble.append(&mut sub_nibble);
                        Self::collapse(database, cache,
                                       MerkleNode::Extension(new_sub_nibble, sub_value))
                    },
                    _ => MerkleNode::Extension(node_nibble, node_value),
                }
            },
            MerkleNode::Branch(node_nodes, node_additional) => {
                let value_count = node_additional.iter().count() +
                    node_nodes.iter().filter(|v| v != &&MerkleValue::Empty).count();

                if value_count == 0 {
                    panic!()
                } else if value_count > 1 {
                    MerkleNode::Branch(node_nodes, node_additional)
                } else if node_additional.is_some() /* value_count == 1 */ {
                    MerkleNode::Leaf(NibbleVec::new(), node_additional.unwrap())
                } else /* value_count == 1, value in nodes */ {
                    let (value_index, value) = node_nodes
                        .iter().enumerate().filter(|&(_, value)| {
                            value != &MerkleValue::Empty
                        }).next()
                        .map(|(value_index, value)| (value_index, value.clone())).unwrap();
                    let value_nibble: Nibble = value_index.into();

                    let subnode = find_subnode(database, cache, value.clone());
                    match subnode {
                        MerkleNode::Leaf(mut sub_nibble, sub_value) => {
                            sub_nibble.insert(0, value_nibble);
                            MerkleNode::Leaf(sub_nibble, sub_value)
                        },
                        MerkleNode::Extension(mut sub_nibble, sub_value) => {
                            sub_nibble.insert(0, value_nibble);
                            Self::collapse(database, cache,
                                           MerkleNode::Extension(sub_nibble, sub_value))
                        },
                        MerkleNode::Branch(sub_nodes, sub_additional) => {
                            Self::collapse(database, cache,
                                           MerkleNode::Extension(vec![value_nibble], value))
                        },
                    }
                }
            },
        }
    }

    fn remove_by_node<'a, 'b: 'a>(
        database: &mut Change<'a, D>, cache: &'a Cache,
        nibble: NibbleVec, node: MerkleNode<'a>
    ) -> Option<MerkleNode<'a>> {
        match node {
            MerkleNode::Leaf(ref node_nibble, ref node_value) => {
                if *node_nibble == nibble {
                    None
                } else {
                    Some(MerkleNode::Leaf(node_nibble.clone(), node_value.clone()))
                }
            },
            MerkleNode::Extension(ref node_nibble, ref node_value) => {
                if nibble.starts_with(node_nibble) {
                    let value = Self::remove_by_value(
                        database, cache,
                        nibble.split_at(node_nibble.len()).1.into(),
                        node_value.clone());
                    Some(Self::collapse(database, cache,
                                        MerkleNode::Extension(node_nibble.clone(), value)))
                } else {
                    Some(MerkleNode::Extension(node_nibble.clone(), node_value.clone()))
                }
            },
            MerkleNode::Branch(ref node_nodes, ref node_additional) => {
                let mut nodes = Self::copy_nodes(node_nodes);
                let mut additional = node_additional.clone();

                if nibble.len() > 0 {
                    let nibble_index: usize = nibble[0].into();
                    nodes[nibble_index] = Self::remove_by_value(
                        database, cache,
                        nibble.split_at(1).1.into(),
                        nodes[nibble_index].clone());
                } else {
                    additional = None;
                }

                let value_count = additional.iter().count() +
                    nodes.iter().filter(|v| v != &&MerkleValue::Empty).count();

                if value_count == 0 {
                    None
                } else {
                    Some(Self::collapse(database, cache, MerkleNode::Branch(nodes, additional)))
                }
            },
        }
    }

    pub fn remove<'a, 'b: 'a>(&'a mut self, key: &'b [u8]) {
        if self.is_empty() {
            return;
        }

        let (changeset, root_rlp) = {
            let cache = Cache::new();
            let mut change = Change::new(&self.database);
            let nibble = nibble::from_key(key);
            let dbv = match self.database.get(self.root) {
                Some(val) => val,
                None => panic!(),
            };
            let node = cache.insert(self.root, dbv);
            let root_rlp = Self::remove_by_node(&mut change, &cache, nibble, node).map(|v| rlp::encode(&v).to_vec());
            (ChangeSet::from(change), root_rlp)
        };
        changeset.drain(&mut self.database, true);
        if root_rlp.is_some() {
            let root_rlp = root_rlp.unwrap();
            let hash = H256::from(Keccak256::digest(&root_rlp).as_slice());
            self.database.set(hash, root_rlp);
            self.root = hash;
        } else {
            self.root = empty_trie_hash();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Database, Trie};
    use std::collections::HashMap;
    use std::str::FromStr;
    use std::cell::UnsafeCell;
    use bigint::H256;
    use hexutil::read_hex;

    #[test]
    fn trie_middle_leaf() {
        let mut map = HashMap::new();
        map.insert("key1aa".as_bytes().into(), "0123456789012345678901234567890123456789xxx".as_bytes().into());
        map.insert("key1".as_bytes().into(), "0123456789012345678901234567890123456789Very_Long".as_bytes().into());
        map.insert("key2bb".as_bytes().into(), "aval3".as_bytes().into());
        map.insert("key2".as_bytes().into(), "short".as_bytes().into());
        map.insert("key3cc".as_bytes().into(), "aval3".as_bytes().into());
        map.insert("key3".as_bytes().into(), "1234567890123456789012345678901".as_bytes().into());

        let mut database: HashMap<H256, Vec<u8>> = HashMap::new();
        let mut trie: Trie<HashMap<H256, Vec<u8>>> = Trie::build(database, &map);

        assert_eq!(trie.root(), H256::from_str("0xcb65032e2f76c48b82b5c24b3db8f670ce73982869d38cd39a624f23d62a9e89").unwrap());
        assert_eq!(trie.get("key2bb".as_bytes()), Some("aval3".as_bytes().into()));
        assert_eq!(trie.get("key2bbb".as_bytes()), None);
        let prev_hash = trie.root();
        trie.insert("key2bbb".as_bytes().into(), "aval4".as_bytes().into());
        assert_eq!(trie.get("key2bbb".as_bytes()), Some("aval4".as_bytes().into()));
        trie.remove("key2bbb".as_bytes());
        assert_eq!(trie.get("key2bbb".as_bytes()), None);
        assert_eq!(prev_hash, trie.root());
    }

    #[test]
    fn insert_middle_leaf() {
        let mut database: HashMap<H256, Vec<u8>> = HashMap::new();
        let mut trie = Trie::empty(database);

        trie.insert("key1aa".as_bytes().into(),
                    "0123456789012345678901234567890123456789xxx".as_bytes().into());
        trie.insert("key1".as_bytes().into(),
                    "0123456789012345678901234567890123456789Very_Long".as_bytes().into());
        trie.insert("key2bb".as_bytes().into(),
                    "aval3".as_bytes().into());
        trie.insert("key2".as_bytes().into(),
                    "short".as_bytes().into());
        trie.insert("key3cc".as_bytes().into(),
                    "aval3".as_bytes().into());
        trie.insert("key3".as_bytes().into(),
                    "1234567890123456789012345678901".as_bytes().into());
        assert_eq!(trie.root(), H256::from_str("0xcb65032e2f76c48b82b5c24b3db8f670ce73982869d38cd39a624f23d62a9e89").unwrap());
    }

    #[test]
    fn insert_single_item() {
        let mut database: HashMap<H256, Vec<u8>> = HashMap::new();
        let mut trie = Trie::empty(database);

        trie.insert("A".as_bytes().into(),
                    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".as_bytes().into());

        assert_eq!(trie.root(), H256::from_str("0xd23786fb4a010da3ce639d66d5e904a11dbc02746d1ce25029e53290cabf28ab").unwrap());
    }

    #[test]
    fn testy() {
        let mut database: HashMap<H256, Vec<u8>> = HashMap::new();
        let mut trie = Trie::empty(database);

        trie.insert("test".as_bytes().into(),
                    "test".as_bytes().into());
        trie.insert("te".as_bytes().into(),
                    "testy".as_bytes().into());

        assert_eq!(trie.root(), H256::from_str("0x8452568af70d8d140f58d941338542f645fcca50094b20f3c3d8c3df49337928").unwrap());
    }

    #[test]
    fn sub_genesis() {
        let mut database: HashMap<H256, Vec<u8>> = HashMap::new();
        let mut trie = Trie::empty(database);

        let k1 = read_hex("0x204188718653cd7e50f3fd51a820db66112517ca190c637e7cdd80782d56").unwrap();
        let v1 = vec![248, 78, 128, 138, 21, 45, 2, 199, 225, 74, 246, 128, 0, 0, 160, 86, 232, 31, 23, 27, 204, 85, 166, 255, 131, 69, 230, 146, 192, 248, 110, 91, 72, 224, 27, 153, 108, 173, 192, 1, 98, 47, 181, 227, 99, 180, 33, 160, 197, 210, 70, 1, 134, 247, 35, 60, 146, 126, 125, 178, 220, 199, 3, 192, 229, 0, 182, 83, 202, 130, 39, 59, 123, 250, 216, 4, 93, 133, 164, 112];
        let k2 = read_hex("0xa390953f116afb00f89fbedb2f8e77297e4e7e1749e2ef0e32e17808e4ad").unwrap();
        let v2 = vec![248, 77, 128, 137, 108, 107, 147, 91, 139, 189, 64, 0, 0, 160, 86, 232, 31, 23, 27, 204, 85, 166, 255, 131, 69, 230, 146, 192, 248, 110, 91, 72, 224, 27, 153, 108, 173, 192, 1, 98, 47, 181, 227, 99, 180, 33, 160, 197, 210, 70, 1, 134, 247, 35, 60, 146, 126, 125, 178, 220, 199, 3, 192, 229, 0, 182, 83, 202, 130, 39, 59, 123, 250, 216, 4, 93, 133, 164, 112];

        trie.insert(k1, v1);
        trie.insert(k2, v2);

        assert_eq!(trie.root(), H256::from_str("bcb5ffb5c6c3e43ef07550fa30af86d66b4015ee3f64aaf70cd0bf8fcc60a9c6").unwrap());
    }

    #[test]
    fn trie_insert() {
        let mut map = HashMap::new();

        let mut database: HashMap<H256, Vec<u8>> = HashMap::new();
        let mut trie: Trie<HashMap<H256, Vec<u8>>> = Trie::build(database, &map);

        trie.insert("foo".as_bytes().into(), "bar".as_bytes().into());
        trie.insert("food".as_bytes().into(), "bass".as_bytes().into());

        assert_eq!(trie.root(), H256::from_str("0x17beaa1648bafa633cda809c90c04af50fc8aed3cb40d16efbddee6fdf63c4c3").unwrap());
    }

    #[test]
    fn trie_delete() {
        let mut map = HashMap::new();

        let mut database: HashMap<H256, Vec<u8>> = HashMap::new();
        let mut trie: Trie<HashMap<H256, Vec<u8>>> = Trie::build(database, &map);

        trie.insert("fooa".as_bytes().into(), "bar".as_bytes().into());
        trie.insert("food".as_bytes().into(), "bass".as_bytes().into());
        let prev_hash = trie.root();
        trie.insert("fooc".as_bytes().into(), "basss".as_bytes().into());
        trie.remove("fooc".as_bytes());
        assert_eq!(trie.root(), prev_hash);
    }

    #[test]
    fn trie_empty() {
        let mut map = HashMap::new();

        let mut database: HashMap<H256, Vec<u8>> = HashMap::new();
        let mut trie: Trie<HashMap<H256, Vec<u8>>> = Trie::build(database, &map);

        assert_eq!(H256::from("0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"),
                   trie.root());
    }
}
