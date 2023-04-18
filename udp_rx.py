#!/usr/bin/env python3
import argparse
import socket, time
import numpy as np
import struct
import io

REPLAY_MODULE = 'replay'
PONG_PORT_INC = 1024

def extract(buffer):
    seq, offset, _length, _port, timestamp = struct.unpack(
        '<IHHHd', buffer[:18])
    return (timestamp, seq, offset)

def main(args):
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.bind(('', args.port))
    if args.calc_rtt:
        import shutil
        import sys
        from pathlib import Path
        import platform
        ##
        module_flags = ['release', 'debug']
        platform_map = {
            'Linux': (f'lib{REPLAY_MODULE}.so',f'{REPLAY_MODULE}.so'),
            'Darwin': (f'lib{REPLAY_MODULE}.dylib',f'{REPLAY_MODULE}.so'),
            'Windows':(f'{REPLAY_MODULE}.dll',f'{REPLAY_MODULE}.pyd') }
        module_file = platform_map[ platform.system() ]
        ##
        for _flag in module_flags:
            if Path(f'target/{_flag}/{module_file[0]}').exists():
                sys.path.append( (Path.cwd()/'target'/_flag).as_posix() )
                shutil.copy2(f'target/{_flag}/{module_file[0]}', f'target/{_flag}/{module_file[1]}')
                break
        ##
        try:
            if platform.system()!='Windows':
                raise Exception('Bypass other unix-like OS.')
            m_replay = __import__(REPLAY_MODULE)
        except:
            print('PongSocket: use unix-like socket.')
            pong_sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
            sock.setsockopt(socket.IPPROTO_IP, socket.IP_TOS, args.tos)
        else:
            print('PongSocket: use system-dependent socket.')
            pong_sock = m_replay.PriorityTxSocket( args.tos )
        pong_port = args.port + PONG_PORT_INC

    received_length = 0
    received_record = {}

    print('waiting ...')
    _buf = sock.recv(10240)
    if args.calc_jitter:
        timestamp, init_seq, _ = extract( _buf )
        received_record[init_seq] = ( timestamp, time.time() )
    sock.setblocking(False)
    init_time = time.time()
    print('started.')

    while time.time()-init_time < args.duration:
        try:
            _buffer, addr = sock.recvfrom(10240)
        except io.BlockingIOError:
            continue
        received_length += len(_buffer)
        ##
        if args.calc_jitter:
            timestamp, seq, offset = extract(_buffer)
            if seq not in received_record:
                received_record[seq] = ( timestamp, time.time() )
            if offset==0: #end of packet
                if args.calc_rtt:
                    duration = time.time() - received_record[seq][1]
                    _buffer = bytearray(_buffer)
                    _buffer[10:18] = struct.pack('d', duration)
                    pong_addr = (addr[0], pong_port)
                    pong_sock.sendto(_buffer, pong_addr)
                received_record[seq] = time.time() - received_record[seq][0]
        pass

    average_throughput_Mbps = (received_length*8/1E6) / args.duration
    print( 'Average Throughput: {:.3f} Mbps'.format(average_throughput_Mbps) )

    if args.calc_jitter:
        average_delay_ms = list(zip( *sorted( received_record.items(), key=lambda x:x[0]) ))[1][:-1]
        average_delay_ms = np.array([ x*1E3 for x in average_delay_ms if type(x)==float ])
        average_jitter_ms = np.diff(average_delay_ms).mean()
        print( 'Average Jitter: {:.6f} ms'.format(average_jitter_ms) )
    pass

if __name__=='__main__':
    parser = argparse.ArgumentParser()
    ##
    parser.add_argument('-p', '--port', type=int, help='binding port for receiving.')
    parser.add_argument('-t', '--duration', type=int,
        help='receiving time duration (unit: second).')
    parser.add_argument('--calc-jitter', action='store_true')
    parser.add_argument('--calc-rtt', action='store_true')
    parser.add_argument('--tos', type=int, default=0, help='set ToS for pong socket.')
    ##
    args = parser.parse_args()
    main(args)
