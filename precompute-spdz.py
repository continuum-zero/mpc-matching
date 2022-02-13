#!/usr/bin/env python3

import argparse
import json
import os

parser = argparse.ArgumentParser(description='Generate SPDZ parameters for testing.')
parser.add_argument('--dir', type=str, default='test-env', help='test-env directory')
parser.add_argument('--beaver-triples', type=int, default=1000000, help='number of beaver triples to be generated')
parser.add_argument('--random-bits', type=int, default=1000000, help='number of random bits to be generated')
parser.add_argument('--input-masks', type=int, default=100, help='number of input masks to be generated')
args = parser.parse_args()

with open(f'{args.dir}/common/config.json', 'r') as config_file:
    config = json.load(config_file)
    num_parties = len(config['parties'])

if os.system('cargo build --release -p dealer') != 0:
    exit(1)

os.system(
    f'./target/release/dealer --parties {num_parties} --output "{args.dir}/node#/precomp.bin" ' +
    f'--beaver-triples {args.beaver_triples} --random-bits {args.random_bits} --input-masks {args.input_masks}'
)
