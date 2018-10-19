from setuptools import setup
from setuptools_rust import Binding, RustExtension

setup(
    name='portus',
    version='4.0',
    description='Python bindings for Portus implementation of CCP',
    url='http://github.com/ccp-project/portus',
    author='Frank Cangialosi',
    author_email='frankc@csail.mit.edu',
    rust_extensions=[RustExtension(
        'portus.pyportus',
        'Cargo.toml',
        binding=Binding.PyO3
    )],
    packages=['portus'],
    license='MIT',
    zip_safe=False
)
