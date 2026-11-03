# Repository rules

- Keep teacher, student, and parent as separate applications with separate identifiers, credentials, capabilities, and local data partitions.
- Do not add an administrator install app.
- Keep the online service and its operational database outside this repository. Never copy online secrets or production records here.
- Default installed games to zero. Install game modules individually from the approved marketplace contract.
- Keep offline data after credential expiry; expiry switches to read/repair/recovery access and must not erase drafts or pending submissions.
- Do not expose answer keys or other students' data to the student app.
- Do not claim a platform or recovery guarantee until its target build and validation evidence exist.
- Avoid network-required setup or runtime dependencies during class.
- Do not run tests unless the user asks to test or verify implementation.