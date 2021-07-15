from .pyportus import DatapathInfo, PyDatapath, PyReport, start_inner
from abc import ABCMeta, abstractmethod
import signal
import sys
import inspect
from . import util, checker

### Class ###
cong_alg_method_signatures = {
    'datapath_programs' : ['self'],
    'new_flow' : ['self', 'datapath', 'datapath_info'],
}

class AlgBase(object):
    @classmethod
    def assert_implements_interface(cls, C):
        if cls is AlgBase:
            for m in cong_alg_method_signatures.keys():
                if not m in C.__dict__:
                    raise NotImplementedError(
                        "{} does not implement the required method {}".format(
                            C.__name__,
                            m
                        ))
                if inspect.getargspec(getattr(C, m)).args != cong_alg_method_signatures[m]:
                    raise NameError(
                        "{}.{} does not match the required parameters {}".format(
                            C.__name__,
                            m,
                            '(' + ', '.join(cong_alg_method_signatures[m]) + ')'
                        ))
            return True

def start(ipc, alg):
    cls = alg.__class__
    if not issubclass(cls, object):
        raise Exception(cls.__name__ + " must be a subclass of object")
    if issubclass(cls, AlgBase):
        AlgBase.assert_implements_interface(cls)
        checker._check_datapath_programs(cls)
        return start_inner(ipc, alg)
    else:
        raise Exception(cls.__name__ + " must be a subclass of portus.AlgBase")
