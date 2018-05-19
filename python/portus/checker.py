from .pyportus import _try_compile
import sys
import ast, _ast
import inspect

class DatapathProgram(object):
    def __init__(self, code, lineno):
        self.code = code
        self.lineno = lineno

class ProgramFinder(ast.NodeVisitor):
    def __init__(self, src_file_name):
        self.src_file_name = src_file_name
        self.progs = []

    def visit_Call(self, call_node):
        for child in ast.walk(call_node):
            if isinstance(child, _ast.Attribute) and child.attr == "install":
                args = call_node.args
                if len(args) > 2:
                    raise ValueError("datapath.install expects a datapath program string and (optionally) a list of tuples of fields to set at the same time")
                arg = args[0]
                found_string_arg = False
                for arg_child in ast.walk(arg):
                    if isinstance(arg_child, _ast.Str):
                        self.progs.append(DatapathProgram(arg_child.s, child.lineno))
                        found_string_arg = True
                if not found_string_arg:
                    raise ValueError("datapath.install must be passed a datapath program as a string literal (not a variable defined elsewhere)")

def _find_datapath_programs(cls):
    src_file_name = inspect.getfile(cls)
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
    UNDERLINE = '\033[4m'

def bold_red_text(t):
    return Colors.BOLDRED + t + Colors.END
def bold_text(t):
    return Colors.BOLD + t + Colors.END

def _check_datapath_programs(cls):
    any_errors = False
    pf = _find_datapath_programs(cls)
    for prog in pf.progs:
        ret = _try_compile(prog.code)
        if ret != "":
            any_errors = True
            sys.stderr.write("Traceback (datapath program compile error):\n  File \"{}\", line {}\n{}\n{}: {}\n".format(
                pf.src_file_name,
                prog.lineno,
                prog.code,
                bold_red_text("error"),
                bold_text(ret)
            ))
            sys.stderr.write("CCP not started. You must fix datapath program compile errors first.\n")

    if any_errors:
        sys.exit(-1)
