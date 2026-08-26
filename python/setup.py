#!/usr/bin/env python3

import os
import sys
from setuptools import setup
from pyo3_setuptools import Pyo3Extension, build_ext

def get_long_description():
    with open("README.md", "r", encoding="utf-8") as fh:
        return fh.read()

def get_version():
    # Try to read from local Cargo.toml first, then parent
    for cargo_path in ["Cargo.toml", "../Cargo.toml"]:
        try:
            with open(cargo_path, "r") as f:
                for line in f:
                    if line.startswith("version = "):
                        return line.split('"')[1]
        except FileNotFoundError:
            continue
    return "0.1.0"

ext_modules = [
    Pyo3Extension(
        "pdf_ast",
        [
            "src/lib.rs",
        ],
        crate_path=".",
        rust_version=">=1.85",
        py_limited_api=True,
    ),
]

setup(
    name="pdf-ast",
    version=get_version(),
    author="Marc Rivero López",
    description="Experimental PDF AST bindings for structural and security analysis",
    long_description=get_long_description(),
    long_description_content_type="text/markdown",
    url="https://github.com/seifreed/pdf-core",
    project_urls={
        "Bug Tracker": "https://github.com/seifreed/pdf-core/issues",
        "Source Code": "https://github.com/seifreed/pdf-core",
    },
    ext_modules=ext_modules,
    cmdclass={"build_ext": build_ext},
    zip_safe=False,
    python_requires=">=3.7",
    classifiers=[
        "Development Status :: 3 - Alpha",
        "Intended Audience :: Developers",
        "Intended Audience :: Science/Research",
        "License :: OSI Approved :: MIT License",
        "Operating System :: OS Independent",
        "Programming Language :: Python :: 3",
        "Programming Language :: Python :: 3.7",
        "Programming Language :: Python :: 3.8",
        "Programming Language :: Python :: 3.9",
        "Programming Language :: Python :: 3.10",
        "Programming Language :: Python :: 3.11",
        "Programming Language :: Python :: 3.12",
        "Programming Language :: Rust",
        "Topic :: Scientific/Engineering",
        "Topic :: Software Development :: Libraries :: Python Modules",
        "Topic :: Text Processing",
        "Topic :: Office/Business",
    ],
    keywords=[
        "pdf", "ast", "document", "parsing", "analysis", 
        "iso32000", "pdf2.0", "validation", "security",
        "malware", "forensics", "document-processing"
    ],
    install_requires=[
        "typing-extensions; python_version<'3.8'",
    ],
    extras_require={
        "dev": [
            "pytest>=6.0",
            "pytest-benchmark",
            "black",
            "mypy",
            "ruff",
        ],
        "docs": [
            "sphinx",
            "sphinx-rtd-theme",
            "myst-parser",
        ],
    },
)
