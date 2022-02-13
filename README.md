# mpc-matching

Maximizing average happiness privately & securely.

## What?

Suppose there's a group of `n` ladies and `n` lads, and they want to be matched in pairs.
Each person has some deeply hidden desires, represented by an integer vector.
For each pair, cost of matching them together is a function of their desires.
We want to find a matching that minimizes the average cost, without revealing their secrets - each person should learn only who is their better half.

## How?

Oblivious minimum cost maximum flow algorithm running under SPDZ protocol.

## What's included?

- `mpc` library - mini-framework for MPC computation (SPDZ online phase, fundamental circuits and networking)
- `mpc_flow` library - implementation of oblivious minimum cost flow and matching algorithms for use in MPC
- `dealer` - tool that precomputes stuff for SPDZ protocol
- `mpc_test_app` - the secret matching application

## Prerequisities

1. Rust 1.58 - to compile the projects
2. Python 3.8 - for convenience scripts
2. OpenSSL - for generating self-signed certificates

## Running

1. Build everything: `cargo build --release`
2. Create test environment for 16 nodes: `./prepare-test-env.py`
3. Precompute parameters for SPDZ: `./precompute-spdz.py`
4. Run all test nodes locally: `./run-all-parties.py`

You can run test nodes individually using `./run-party.py`, run scripts with `--help` for more information.
