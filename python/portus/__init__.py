from .pyportus import _connect, DatapathInfo, PyDatapath, PyReport
from abc import ABCMeta, abstractmethod
import inspect
import signal
import sys

### Class ### 
method_signatures = {
    '__init__' : ['self', 'datapath', 'datapath_info'],
    'on_report' : ['self', 'r']
}
class AlgBase:
    __metaclass__ = ABCMeta
    @abstractmethod
    def __init__(self):
        #raise NotImplementedError
        return NotImplemented
    @abstractmethod
    def on_report(self):
        #raise NotImplementedError
        return NotImplemented
    @classmethod
    def implements_interface(cls, C):
        if cls is AlgBase:
            for m in method_signatures.keys():
                if not m in C.__dict__:
                    raise NotImplementedError(
                        "{} does not implement the required method {}".format(
                            C.__name__,
                            m
                        ))
                if inspect.getargspec(getattr(C, m)).args != method_signatures[m]:
                    raise NameError(
                        "{}.{} does not match the required parameters {}".format(
                            C.__name__,
                            m,
                            '(' + ', '.join(method_signatures[m]) + ')'
                        ))
            return True

def connect(ipc, cls, blocking=True, debug=False):
    if not issubclass(cls, object):
        raise Exception(cls.__name__ + " must be a subclass of object")
    if not issubclass(cls, AlgBase):
        raise Exception(cls.__name__ + " must be a subclass of portus.AlgBase")
    if not AlgBase.implements_interface(cls):
        return
    AlgBase.register(cls)

    return _connect(ipc, cls, blocking, debug)

### Utils ###
def ip_to_str(ip):
    import struct, socket
    return struct.unpack("!I", socket.inet_aton(ip))[0]
def str_to_ip(s):
    import struct, socket
    return socket.inet_ntoa(struct.pack("!I", s))
