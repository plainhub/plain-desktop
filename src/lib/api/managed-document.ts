// The gateway rejects credential fields rather than returning a peer's keys.
export function managedGraphqlDocument(document: string): string {
  return document
    .replace(/(fragment AppFragment on App\s*\{\s*clientId\s*)urlToken\b/, '$1')
    .replace(/(\.\.\. on ChatShareContent\s*\{\s*__typename\s*shareId\s*)urlToken\b/g, '$1')
}
