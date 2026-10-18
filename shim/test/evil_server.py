#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Hostile stand-in for tsubad: answers every connection with the next malformed reply.

    evil_server.py PORTFILE COUNTFILE   listen on 127.0.0.1:<ephemeral>, write the port to
                                        PORTFILE and the number of replies served to COUNTFILE

The bridge must refuse every one of these (log "(-1)", never open a session) without crashing or
blocking the game. Case 0 is the reply that overflowed the old chunked decoder's heap buffer.
"""
import socket
import sys
import threading
import time

CASES = [
    # Chunk size larger than the data: the old decoder memcpy'd 64 KiB from/to small buffers.
    b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n10000\r\nAB\r\n0\r\n\r\n",
    # Chunk size that wraps size arithmetic.
    b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nffffffffffffffff\r\nAB\r\n0\r\n\r\n",
    # Chunk without its CRLF, valid-looking terminator.
    b'HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\n{"uri"\r\n0\r\n\r\n',
    # Upper-case / spaced transfer coding.
    b"HTTP/1.1 200 OK\r\nTransfer-Encoding:   CHUNKED\r\n\r\n2\r\n{}\r\n0\r\n\r\n",
    # Negative and absurd lengths.
    b"HTTP/1.1 200 OK\r\nContent-Length: -5\r\n\r\n" + b"X" * 100,
    b"HTTP/1.1 200 OK\r\nContent-Length: 99999999999\r\n\r\nX",
    b"HTTP/1.1 200 OK\r\nContent-Length: abc\r\n\r\n{}",
    # Body shorter than announced (then close).
    b'HTTP/1.1 200 OK\r\nContent-Length: 300\r\n\r\n{"uri":"http://localhost:1/x"',
    # Not HTTP at all.
    b"\x00\xff\x00garbage\r\n\r\n",
    # Head that never ends (> 1 MiB).
    b"HTTP/1.1 200 OK\r\nX: " + b"A" * (1100 * 1024),
]


def handle(c, case):
    try:
        c.settimeout(3)
        c.recv(65536)
        c.sendall(CASES[case])
        time.sleep(0.2)
    except OSError:
        pass
    finally:
        c.close()


def main(portfile, countfile):
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    s.listen(16)
    with open(portfile, "w") as f:
        f.write(str(s.getsockname()[1]))
    served = 0
    while True:
        c, _ = s.accept()
        case = served % len(CASES)
        served += 1
        threading.Thread(target=handle, args=(c, case), daemon=True).start()
        with open(countfile, "w") as f:
            f.write(str(served))


if __name__ == "__main__":
    if sys.argv[1] == "cases":
        print(len(CASES))
    else:
        main(sys.argv[1], sys.argv[2])
