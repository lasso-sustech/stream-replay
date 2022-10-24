#!/usr/bin/env python3
import argparse
import socket, time

def extract(buffer):
    seq = int.from_bytes(buffer[0:4])
    offset = int.from_bytes(buffer[4:5])
    return (seq, offset)

def main(args):
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.bind(('', args.port))
    received_length = 0
    received_record = {}

    init_seq, _ = extract( sock.recv(10240) )
    received_record[init_seq] = time.time()
    init_time = time.time()

    while time.time()-init_time < args.duration:
        _buffer = sock.recv(10240)
        received_length += len(_buffer)
        ##
        if args.calc_delay:
            seq, offset = extract(_buffer)
            if seq not in received_record:
                received_record[seq] = init_time + (seq-init_seq)*(args.interval*1E-3)
            elif offset==0:
                received_record[seq] = time.time() - received_record[seq]
        pass

    average_throughput_Mbps = (received_length*8/1E6) / args.duration
    print( 'Average Throughput: {:.3f} Mbps'.format(average_throughput_Mbps) )

    if args.calc_delay:
        _records = [x for x in received_record if x < time.time()]
        average_delay_ms = sum(_records)/len(_records) * 1E3
        print( 'Average Delay: {:.3f} ms'.format(average_delay_ms) )
    pass

if __name__=='__main__':
    parser = argparse.ArgumentParser()
    ##
    parser.add_argument('-p', '--port', type=int, help='binding port for receiving.')
    parser.add_argument('-t', '--duration', type=int,
        help='receiving time duration (unit: second).')
    parser.add_argument('--interval', type=float, nargs='?', default=0,
        help='packet send interval, used for delay calculation (unit: ms).')
    parser.add_argument('--calc-delay', action='store_true')
    ##
    args = parser.parse_args()
    main(args)
