from setuptools import setup
from setuptools_rust import Binding, RustExtension

setup(
    name='portus',
    version='0.1',
    rust_extensions=[RustExtension(
        'portus.pyportus',
        'Cargo.toml',
        binding=Binding.PyO3
    )],
    packages=['portus'],
    zip_safe=False
)
