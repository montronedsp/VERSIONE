# Common storage notes

- `ObjectId` is a content digest, not a URL.
- Replicas are tracked by `StorageLocationId`.
- Prefer verify-before-publish and never drop the last valid replica automatically.
