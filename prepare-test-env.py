#!/usr/bin/env python3

import argparse
import json
import os

parser = argparse.ArgumentParser(description='Generate config and self-signed certificates for testing.')
parser.add_argument('--dir', type=str, default='test-env', help='target directory')
parser.add_argument('--parties', type=int, default=3, help='number of parties')
parser.add_argument('--address', type=str, default='127.0.0.1', help='address on which all parties listen')
parser.add_argument('--base-port', type=int, default=5000, help='port of the first party')
args = parser.parse_args()

if os.path.isdir(args.dir):
    print(f'Target directory `{args.dir}` already exists, please remove it first!')
    exit(1)

os.mkdir(args.dir)
os.mkdir(f'{args.dir}/common')

config = {'parties': []}

for i in range(args.parties):
    config['parties'].append({
        'address': f'{args.address}:{args.base_port + i}',
        'certificate': f'node{i}.pem',
    })

    os.mkdir(f'{args.dir}/node{i}')

    status = os.system(
        f'openssl req -newkey rsa:2048 -nodes -keyout "{args.dir}/node{i}/private.key" -x509 -days 365 ' +
        f'-out "{args.dir}/common/node{i}.pem" -subj "/" -config openssl.cnf -extensions v3_req'
    )

    if status != 0:
        exit(1)

with open(f'{args.dir}/common/config.json', 'w') as config_file:
    config_file.write(json.dumps(config, indent=4))
