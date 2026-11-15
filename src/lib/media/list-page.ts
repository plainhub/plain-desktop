// A refresh replaces the loaded window, while scrolling fetches only the next page.
export function mediaListPage(page: number, pageSize: number, refreshLoaded: boolean) {
  return refreshLoaded
    ? { offset: 0, limit: page * pageSize }
    : { offset: (page - 1) * pageSize, limit: pageSize }
}
