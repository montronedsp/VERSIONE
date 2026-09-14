# S3-compatible object store (planned)

Provider-agnostic S3 API target (AWS S3, B2, Wasabi, R2, MinIO, …).

Do not hard-code one cloud vendor into the common `ObjectStore` trait. Credentials must not be stored in Git-tracked `.versione/` files.
