#!/usr/bin/env python3
import argparse
import socket, time
import numpy as np

def extract(buffer):
    seq = int.from_bytes(buffer[0:4], 'little')
    offset = int.from_bytes(buffer[4:6], 'little')
    return (seq, offset)

def main(args):
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.bind(('', args.port))
    pong_sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    pong_port = args.port + 1024

    received_length = 0
    received_record = {}

    print('waiting ...')
    init_seq, _ = extract( sock.recv(10240) )
    received_record[init_seq] = time.time()
    init_time = time.time()
    print('started.')

    while time.time()-init_time < args.duration:
        _buffer, addr = sock.recvfrom(10240)
        seq, offset = extract(_buffer)
        received_length += len(_buffer)
        ##
        if args.calc_rtt:
            pong_addr = (addr[0], pong_port)
            pong_sock.sendto(_buffer[0:4], pong_addr)
        ##
        if args.calc_jitter:
            if offset==0: #end of packet
                received_record[seq] = time.time()
        pass

    average_throughput_Mbps = (received_length*8/1E6) / args.duration
    print( 'Average Throughput: {:.3f} Mbps'.format(average_throughput_Mbps) )

    if args.calc_jitter:
        timestamps = list(zip( *sorted( received_record.items(), key=lambda x:x[0]) ))
        average_jitter_ms = np.diff(timestamps).mean() * 1E3
        print( 'Average Jitter: {:.3f} ms'.format(average_jitter_ms) )
    pass

if __name__=='__main__':
    parser = argparse.ArgumentParser()
    ##
    parser.add_argument('-p', '--port', type=int, help='binding port for receiving.')
    parser.add_argument('-t', '--duration', type=int,
        help='receiving time duration (unit: second).')
    parser.add_argument('--interval', type=float, nargs='?', default=0,
        help='packet send interval, used for delay calculation (unit: ms).')
    parser.add_argument('--calc-jitter', action='store_true')
    parser.add_argument('--calc-rtt', action='store_true')
    ##
    args = parser.parse_args()
    main(args)
