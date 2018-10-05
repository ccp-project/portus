from .pyportus import _connect, DatapathInfo, PyDatapath, PyReport
from abc import ABCMeta, abstractmethod
import signal
import sys
import inspect
from . import util, checker

### Class ###
method_signatures = {
    'init_programs' : ['self'],
    'on_create' : ['self'],
    'on_report' : ['self', 'r']
}

generic_method_signatures = {
    '__init__' : ['self', 'init_cwnd', 'mss'],
    'curr_cwnd' : ['self'],
    'set_cwnd' : ['self', 'cwnd'],
    'increase': ['self', 'm'],
    'reduction': ['self', 'm'],
}

class AlgBase:
    __metaclass__ = ABCMeta
    @abstractmethod
    def on_create(self):
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

class GenericCongAvoidBase:
    __metaclass__ = ABCMeta
    @abstractmethod
    def __init__(self):
        return NotImplemented
    @abstractmethod
    def curr_cwnd(self):
        return NotImplemented
    @abstractmethod
    def set_cwnd(self, cwnd):
        return NotImplemented
    @abstractmethod
    def increase(self, m):
        return NotImplemented
    @abstractmethod
    def reduction(self, m):
        return NotImplemented
    @classmethod
    def assert_implements_interface(cls, C):
        if cls is GenericCongAvoidBase:
            for m in generic_method_signatures.keys():
                if not m in C.__dict__:
                    raise NotImplementedError(
                        "{} does not implement the required method {}".format(
                            C.__name__,
                            m
                        ))
                if inspect.getargspec(getattr(C, m)).args != generic_method_signatures[m]:
                    raise NameError(
                        "{}.{} does not match the required parameters {}".format(
                            C.__name__,
                            m,
                            '(' + ', '.join(generic_method_signatures[m]) + ')'
                        ))

def start(ipc, cls, config={}, debug=False):
    if not issubclass(cls, object):
        raise Exception(cls.__name__ + " must be a subclass of object")
    if issubclass(cls, AlgBase):
        if not config is None:
            raise Exception("Only algorithms implementing GenericCongAvoidBase use a config")
        AlgBase.assert_implements_interface(cls)
        checker._check_datapath_programs(cls)
        AlgBase.register(cls)
        return _connect(ipc, cls, debug, {})
    elif issubclass(cls, GenericCongAvoidBase):
        default_config = {
            'ss_thresh' : 0x7fffffff,
            'init_cwnd' : 0,
            'report' : 'rtt',
            'ss' : 'ccp',
            'use_compensation' : False,
            'deficit_timeout' : 0,
        }
        default_config.update(config)
        GenericCongAvoidBase.assert_implements_interface(cls)
        return _connect(ipc, cls, debug, default_config)
    else:
        raise Exception(cls.__name__ + " must be a subclass of portus.AlgBase or portus.GenericCongAvoidBase")

