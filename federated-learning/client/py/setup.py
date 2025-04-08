#!/usr/bin/env python
# -*- coding: utf-8 -*-

"""Umazen Python Client - Official SDK for Decentralized AI Infrastructure"""

import pathlib
import sys

from setuptools import setup, find_packages

# Version synchronization
VERSION = "0.8.1"
SOLANA_SDK_VERSION = ">=0.25.0,<0.26"
AI_DEPENDENCIES = [
    "numpy>=1.22.0",
    "torch>=2.0.0",
    "tensorflow>=2.12.0",
    "huggingface_hub>=0.14.0",
]

# Platform-specific requirements
SYSTEM_DEPS = {
    "sys_platform == 'linux'": [
        "libsolana-crypto-helpers>=0.3.0"
    ],
    "sys_platform == 'darwin'": [
        "macos-secure-enclave>=1.2.0"
    ],
}

# Development requirements
DEV_REQUIRES = [
    "black>=23.7.0",
    "mypy>=1.4.0",
    "pytest-asyncio>=0.21.0",
    "pytest-cov>=4.1.0",
    "hypothesis>=6.82.0",
]

setup(
    name="umazen",
    version=VERSION,
    author="Umazen Labs",
    author_email="dev@umazen.ai",
    license="Apache-2.0",
    description="Python SDK for Decentralized AI Infrastructure on Solana",
    long_description=pathlib.Path("README.md").read_text(),
    long_description_content_type="text/markdown",
    url="https://github.com/umazen-labs/client-py",
    project_urls={
        "Documentation": "https://docs.umazen.ai",
        "Source Code": "https://github.com/umazen-labs/client-py",
        "Bug Tracker": "https://github.com/umazen-labs/client-py/issues",
    },
    classifiers=[
        "Development Status :: 4 - Beta",
        "Intended Audience :: Developers",
        "Intended Audience :: Science/Research",
        "License :: OSI Approved :: Apache Software License",
        "Programming Language :: Python :: 3.10",
        "Programming Language :: Python :: 3.11",
        "Topic :: Scientific/Engineering :: Artificial Intelligence",
        "Topic :: Software Development :: Libraries :: Python Modules",
        "Framework :: AsyncIO",
    ],
    python_requires=">=3.10.0,<3.12",
    packages=find_packages(
        where="src",
        include=["umazen", "umazen.*"],
        exclude=["tests", "examples"]
    ),
    package_dir={"": "src"},
    package_data={
        "umazen.abi": ["*.json"],
        "umazen.models": ["*.joblib"],
        "umazen.config": ["*.yaml"],
    },
    include_package_data=True,
    install_requires=[
        f"solana-py{SOLANA_SDK_VERSION}",
        "anchorpy>=1.0.0",
        "cryptography>=40.0.0",
        "aiohttp>=3.8.0",
        "msgpack>=1.0.0",
        "pynacl>=1.5.0",
        "requests>=2.28.0",
        "typing-extensions>=4.0.0",
    ] + AI_DEPENDENCIES,
    extras_require={
        "dev": DEV_REQUIRES,
        "gpu": [
            "nvidia-cuda-runtime-cu11>=11.7.0",
            "cupy-cuda11x>=12.0.0",
        ],
        "quantum": [
            "qiskit>=0.43.0",
            "pennylane>=0.31.0",
        ],
        "ml": [
            "onnxruntime>=1.14.0",
            "scikit-learn>=1.2.0",
        ],
        **SYSTEM_DEPS,
    },
    entry_points={
        "console_scripts": [
            "umazen-cli=umazen.cli.main:app",
            "umazen-train=umazen.training.executor:main",
            "umazen-infer=umazen.inference.server:start_server",
        ]
    },
    scripts=[
        "scripts/model_verifier.py",
        "scripts/chain_monitor.py",
    ],
    zip_safe=False,
    options={
        "build": {
            "build_base": "build",
            "force": True,
        },
        "bdist_wheel": {
            "universal": False,
            "plat_name": "any",
        },
        "egg_info": {
            "tag_build": "",
            "tag_date": False,
        },
    },
)
