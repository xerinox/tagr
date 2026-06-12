# Security Policy

We take the security of Tagr seriously. This document outlines our supported versions, reporting procedures, and security-hardened design principles.

## Supported Versions

Only the latest major release of Tagr is actively supported with security updates.

| Version | Supported          |
| ------- | ------------------ |
| 1.0.x   | :white_check_mark: |
| < 1.0   | :x:                |

## Reporting a Vulnerability

If you discover a security vulnerability in Tagr, please **do not** use the public issue tracker. Instead, report it privately to the maintainer:

- **Email**: rkildal@gmail.com
- Please include:
  - A descriptive title and summary of the issue.
  - Detailed steps to reproduce the vulnerability (including proof-of-concept code or sample files where applicable).
  - An assessment of the potential impact (e.g., local code execution, denial of service).
  - Any proposed remediation or patch.

We will acknowledge receipt of your report within 48 hours and work with you to coordinate a security advisory and publication timeline.

## Safe and Hardened Design Principles

Tagr is designed to be a highly secure utility for local file tagging. We adhere to the following strict security architectural guidelines:

1. **Memory Safety without `unsafe`**:
   The entire Tagr codebase is written in 100% safe Rust. No `unsafe` blocks are allowed in any core modules. This guarantees protection against memory corruption, buffer overflows, and double-free vulnerabilities at compile time.

2. **No Arbitrary Code Execution**:
   When using the execution feature `-x`/`--exec` in interactive browse mode, Tagr executes commands entered as arguments strictly using standard sub-process execution APIs. It does not spawn a full system shell unless explicitly requested, mitigating shell injection issues from raw untrusted filenames.

3. **Database Injection Protection**:
   The database uses `sled` with binary serialization (`postcard`/`bincode`) instead of SQL. There are no parsing logic layers where "un-escaped input" could lead to SQL-injection style privilege escalation or arbitrary database mutation.

4. **Input Validation**:
   All paths are validated for valid UTF-8 and safely encapsulated within typed wrappers (`TagrPath` & `TagName`). Strict validation rejects tag names containing shell interpolation sequences or path-traversal sequences.

5. **Dependency Auditing**:
   We actively use `cargo-deny` and automated dependency scanners in GitHub Actions to check for:
   - Known security advisories (via RustSec).
   - Unwanted/dangerous license configurations.
   - Unauthorized duplicate dependencies.
