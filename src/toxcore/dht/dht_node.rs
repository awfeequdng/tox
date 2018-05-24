/*
    Copyright (C) 2013 Tox project All Rights Reserved.
    Copyright © 2018 Namsoo CHO <nscho66@gmail.com>

    This file is part of Tox.

    Tox is libre software: you can redistribute it and/or modify
    it under the terms of the GNU General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    Tox is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU General Public License for more details.

    You should have received a copy of the GNU General Public License
    along with Tox.  If not, see <http://www.gnu.org/licenses/>.
*/

/*! Data structure used by Bucket.
PackedNode type contains PK and SocketAddress.
PackedNode does not contain status of Node, this struct contains status of node.
Bucket needs status of node, because BAD status node should be replaced with higher proirity than GOOD node.
Even GOOD node is farther than BAD node, BAD node should be replaced.
Here, GOOD node is the node responded within 162 seconds, BAD node is the node not responded over 162 seconds.
*/

use std::net::SocketAddr;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use std::cmp::Ordering;
use toxcore::crypto_core::*;
use toxcore::dht::packed_node::*;
use toxcore::dht::kbucket::*;

/** Status of node in bucket.
Good means it is online and responded within 162 seconds
Bad means it is probably offline and did not responded for over 162 seconds
When new peer is added to bucket, Bad status node should be replace.
If there are no Bad nodes in bucket, node which is farther than peer is replaced.

Manage ping_id.
Generate ping_id on request packet, check ping_id on response packet.
*/
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum NodeStatus {
    /// online
    Good,
    /// maybe offline
    Bad,
}

/// check distance of PK1 and PK2 from base_PK including status of node
pub trait ReplaceOrder {
    /// Check distance of PK1 and Pk2 including status of node
    fn replace_order(&self, &DhtNode, &DhtNode, Duration) -> Ordering;
}

impl ReplaceOrder for PublicKey {
    fn replace_order(&self,
                     node1: &DhtNode,
                     node2: &DhtNode,
                     bad_node_timeout: Duration) -> Ordering {

        trace!(target: "Distance", "Comparing distance between PKs. and status of node");
        match node1.calc_status(bad_node_timeout) {
            NodeStatus::Good => {
                match node2.calc_status(bad_node_timeout) {
                    NodeStatus::Good => { // Good, Good
                        self.distance(&node1.pk, &node2.pk)
                    },
                    NodeStatus::Bad => { // Good, Bad
                        Ordering::Less // Good is closer
                    },
                }
            },
            NodeStatus::Bad => {
                match node2.calc_status(bad_node_timeout) {
                    NodeStatus::Good => { // Bad, Good
                        Ordering::Greater // Bad is farther
                    },
                    NodeStatus::Bad => { // Bad, Bad
                        self.distance(&node1.pk, &node2.pk)
                    },
                }
            },
        }
    }
}
/** Struct used by Bucket, DHT maintains close node list, when we got new node,
we should make decision to add new node to close node list, or not.
the PK's distance and status of node help making decision.
Bad node have higher priority than Good node.
If both node is Good node, then we compare PK's distance.

Generate ping_id on request packet, check ping_id on response packet.
*/
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DhtNode {
    /// Socket addr of node.
    pub saddr: SocketAddr,
    /// Public Key of the node.
    pub pk: PublicKey,
    /// hash of ping_ids to check PingResponse is correct
    pub ping_hash: HashMap<u64, Instant>,
    /// last received ping/nodes-response time
    pub last_resp_time: Instant,
    /// last sent ping-req time
    pub last_ping_req_time: Instant,
}

impl DhtNode {
    /// create DhtNode object
    pub fn new(pn: PackedNode) -> DhtNode {
        DhtNode {
            pk: pn.pk,
            saddr: pn.saddr,
            ping_hash: HashMap::new(),
            last_resp_time: Instant::now(),
            last_ping_req_time: Instant::now(),
        }
    }

    /// calc. status of node
    pub fn calc_status(&self, bad_node_timeout: Duration) -> NodeStatus {
        if self.last_resp_time.elapsed() > bad_node_timeout {
            NodeStatus::Bad
        } else {
            NodeStatus::Good
        }
    }

    /// set new random ping id to the client and return it
    fn generate_ping_id(&mut self) -> u64 {
        loop {
            let ping_id = random_u64();
            if ping_id != 0 && !self.ping_hash.contains_key(&ping_id) {
                return ping_id;
            }
        }
    }

    /// clear timed out ping_id
    pub fn clear_timedout_pings(&mut self, timeout: Duration) {
        self.ping_hash.retain(|&_ping_id, &mut time|
            time.elapsed() <= timeout);
    }

    /// Add a Ping Hash Entry and return a new ping_id.
    pub fn insert_new_ping_id(&mut self) -> u64 {
        let ping_id = self.generate_ping_id();
        self.ping_hash.insert(ping_id, Instant::now());

        ping_id
    }

    /// Check if ping_id is valid and not timed out.
    pub fn check_ping_id(&mut self, ping_id: u64, timeout: Duration) -> bool {
        if ping_id == 0 {
            debug!("Given ping_id is 0");
            return false
        }

        let time_ping_sent = match self.ping_hash.remove(&ping_id) {
            None => {
                debug!("Given ping_id don't exist in PingHash");
                return false
            },
            Some(time) => time,
        };

        if time_ping_sent.elapsed() > timeout {
            debug!("Given ping_id is timed out");
            return false
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck::quickcheck;

    #[test]
    fn client_data_clonable() {
        let pn = PackedNode {
            pk: gen_keypair().0,
            saddr: "127.0.0.1:33445".parse().unwrap(),
        };
        let client = DhtNode::new(pn);
        let _ = client.clone();
    }

    #[test]
    fn client_data_insert_new_ping_id_test() {
        let pn = PackedNode {
            pk: gen_keypair().0,
            saddr: "127.0.0.1:33445".parse().unwrap(),
        };
        let mut client = DhtNode::new(pn);

        let ping_id = client.insert_new_ping_id();

        assert!(client.ping_hash.contains_key(&ping_id));
    }

    #[test]
    fn client_data_check_ping_id_test() {
        let pn = PackedNode {
            pk: gen_keypair().0,
            saddr: "127.0.0.1:33445".parse().unwrap(),
        };
        let mut client = DhtNode::new(pn);

        let ping_id = client.insert_new_ping_id();

        let dur = Duration::from_secs(1);
        // give incorrect ping_id
        assert!(!client.check_ping_id(0, dur));
        assert!(!client.check_ping_id(ping_id + 1, dur));

        // Though ping_id is correct, it is timed-out
        let dur = Duration::from_secs(0);
        assert!(!client.check_ping_id(ping_id, dur));

        // Now, timeout duration is 5 seconds
        let dur = Duration::from_secs(5);

        let ping_id = client.insert_new_ping_id();
        assert!(client.check_ping_id(ping_id, dur));
    }

    #[test]
    fn client_data_clear_timedout_pings_test() {
        let pn = PackedNode {
            pk: gen_keypair().0,
            saddr: "127.0.0.1:33445".parse().unwrap(),
        };
        let mut client = DhtNode::new(pn);

        // ping_id should be removed
        let ping_id = client.insert_new_ping_id();
        let dur = Duration::from_secs(0);
        client.clear_timedout_pings(dur);
        let dur = Duration::from_secs(1);
        assert!(!client.check_ping_id(ping_id, dur));

        // ping_id should remain
        let ping_id = client.insert_new_ping_id();
        let dur = Duration::from_secs(1);
        client.clear_timedout_pings(dur);
        assert!(client.check_ping_id(ping_id, dur));
    }

    #[test]
    fn dht_node_bucket_try_add_test() {
        fn with_nodes(n1: PackedNode, n2: PackedNode, n3: PackedNode,
                      n4: PackedNode, n5: PackedNode, n6: PackedNode,
                      n7: PackedNode, n8: PackedNode) {
            let pk = PublicKey([0; PUBLICKEYBYTES]);
            let mut bucket = Bucket::new(None);
            assert_eq!(true, bucket.try_add(&pk, &n1));
            assert_eq!(true, bucket.try_add(&pk, &n2));
            assert_eq!(true, bucket.try_add(&pk, &n3));
            assert_eq!(true, bucket.try_add(&pk, &n4));
            assert_eq!(true, bucket.try_add(&pk, &n5));
            assert_eq!(true, bucket.try_add(&pk, &n6));
            assert_eq!(true, bucket.try_add(&pk, &n7));
            assert_eq!(true, bucket.try_add(&pk, &n8));

            // updating bucket
            assert_eq!(true, bucket.try_add(&pk, &n1));

            // TODO: check whether adding a closest node will always work
        }
        quickcheck(with_nodes as fn(PackedNode, PackedNode, PackedNode, PackedNode,
                                    PackedNode, PackedNode, PackedNode, PackedNode));
    }
}
