export function verificationCode(digest: Uint8Array): string {
  const value = new DataView(digest.buffer, digest.byteOffset, digest.byteLength).getBigUint64(0)
  return (value % 100_000_000n).toString().padStart(8, '0')
}
