#!/usr/bin/env python3

import argparse
import json
import os
import subprocess
import threading


stdout_lock = threading.Lock()


def redirect_to_stdout(party_id, stream):
    for line in stream:
        line = line.decode('utf-8')
        with stdout_lock:
            print(f'[{party_id:02}] {line}', end='')


parser = argparse.ArgumentParser(description='Run all nodes using generated configuration.')
parser.add_argument('--dir', type=str, default='test-env', help='test-env directory')
args = parser.parse_args()

config_path = f'{args.dir}/common/config.json'

with open(config_path, 'r') as config_file:
    config = json.load(config_file)
    num_parties = len(config['parties'])

if os.system('cargo build --release -p matcher') != 0:
    exit(1)

for party_id in range(num_parties):
    process = subprocess.Popen(
        [
            './target/release/matcher',
            '--config', f'{config_path}',
            '--id', f'{party_id}',
            '--private-key', f'{args.dir}/node{party_id}/private.key',
            '--precomp', f'{args.dir}/node{party_id}/precomp.bin',
            '--preferences', f'{party_id},{party_id+1},{party_id+5}'
        ],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        bufsize=2048
    )
    threading.Thread(target=redirect_to_stdout, args=(party_id, process.stdout)).start()
    threading.Thread(target=redirect_to_stdout, args=(party_id, process.stderr)).start()
