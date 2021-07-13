from .pyportus import try_compile
import sys
import ast
import inspect

class DatapathProgram(object):
    def __init__(self, code, lineno):
        self.code = code
        self.lineno = lineno

class ProgramFinder(ast.NodeVisitor):
    def __init__(self, src_file_name):
        self.src_file_name = src_file_name
        self.progs = []

    def visit_FunctionDef(self, fd_node):
        if fd_node.name == "datapath_programs":
            for elem in fd_node.body:
                if isinstance(elem, ast.Return):
                    ret_node = elem
                    if not isinstance(ret_node.value, ast.Dict):
                        raise ValueError("datapath_programs() must return a dict")
                    for key in ret_node.value.keys:
                        if not isinstance(key, ast.Str):
                            raise ValueError("datapath_programs() must return a dict of **string**->string")
                    for prog in ret_node.value.values:
                        if not isinstance(prog, ast.Str):
                            raise ValueError("datapath_programs() must return a dict of string->**string**")
                        self.progs.append(DatapathProgram(prog.s, prog.lineno))

def _find_datapath_programs(cls):
    src_file_name = inspect.getfile(cls)
    # NOTE: if module is imported, getfile will return the binary (pyc) rather than the source
    # This is a hack that assumes the source file is in the exact same directory
    # (i.e. getfile returns /path/to/x.pyc and we hope the source code is in /path/to/x.py)
    if '.pyc' in src_file_name:
        src_file_name = src_file_name[:-1]
    f = open(src_file_name)
    src = ''.join(f.readlines())
    f.close()
    # src = inspect.getsource(cls)
    tree = compile(src, '<string>', 'exec', ast.PyCF_ONLY_AST)
    pf = ProgramFinder(src_file_name)
    pf.visit(tree)
    return pf


class Colors:
    BLUE = '\033[94m'
    GREEN = '\033[92m'
    ORANGE = '\033[93m'
    RED = '\033[91m'
    END = '\033[0m'
    BOLD = '\033[1m'
    BOLDRED = '\033[91;1m'
    BOLDYELLOW = '\033[93;1m'
    UNDERLINE = '\033[4m'

def bold_yellow_text(t):
    return Colors.BOLDYELLOW + t + Colors.END
def bold_red_text(t):
    return Colors.BOLDRED + t + Colors.END
def bold_text(t):
    return Colors.BOLD + t + Colors.END

def _check_datapath_programs(cls):
    any_errors = False
    pf = _find_datapath_programs(cls)
    if not pf.progs or len(pf.progs) < 1:
        raise ValueError("datapath_programs() must return at least one program!")
    for prog in pf.progs:
        ret = try_compile(prog.code)
        if ret != "":
            any_errors = True
            sys.stderr.write("Traceback (datapath program compile error):\n  File \"{}\", {}\n{}\n{}: {}\n".format(
                pf.src_file_name,
                bold_yellow_text("line " + str(prog.lineno)),
                "|\n|" + "\n|".join(prog.code.split("\n")),
                bold_red_text("error"),
                bold_text(ret)
            ))
            sys.stderr.write("CCP not started. You must fix datapath program compile errors first.\n")

    if any_errors:
        sys.exit(-1)
