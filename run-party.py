#!/usr/bin/env python3

import argparse
import os

parser = argparse.ArgumentParser(description='Run a single node using generated configuration.')
parser.add_argument('--dir', type=str, default='test-env', help='test-env directory')
parser.add_argument('--id', type=int, required=True, help='party ID')
parser.add_argument('--preferences', type=str, required=True, help='preference vector')
args = parser.parse_args()

if os.system('cargo build --release -p matcher') != 0:
    exit(1)

os.system(
    f'./target/release/matcher ' +
    f'--config {args.dir}/common/config.json ' +
    f'--id {args.id} ' +
    f'--private-key {args.dir}/node{args.id}/private.key ' +
    f'--precomp {args.dir}/node{args.id}/precomp.bin ' +
    f'--preferences {args.preferences}'
)
