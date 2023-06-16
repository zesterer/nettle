# Nettle: A Distributed P2P Protocol

Nettle is a peer to peer (P2P) protocol. The design is currently similar to [Kademlia](https://en.wikipedia.org/wiki/Kademlia).

## Goals

- Be independent of the underlying physical network (physical or otherwise)
- Have no single authentication or authorisation authority
- Support hosting both static and dynamic content (the latter via [CRDTs](https://en.wikipedia.org/wiki/Conflict-free_replicated_data_type))
- Automatic peering and route healing
- Redundant storage of data with active work reallocation as nodes enter and leave the network

## Non-goals (for now)

- Spam/DOS resistance
- Security (an attempt has been made, but without guarantees. Do not use this for anything you really care about)
