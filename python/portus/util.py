import struct, socket

def ip_to_str(ip):
    return struct.unpack("!I", socket.inet_aton(ip))[0]
def str_to_ip(s):
    return socket.inet_ntoa(struct.pack("!I", s))