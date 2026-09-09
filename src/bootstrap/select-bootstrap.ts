export interface BootstrapLoaders {
  loadFullApp(): Promise<void>
  loadScreenCapture(): Promise<void>
}

export async function runSelectedBootstrap(pathname: string, isTauri: boolean, loaders: BootstrapLoaders): Promise<void> {
  if (isTauri && pathname === '/screen-capture') {
    await loaders.loadScreenCapture()
    return
  }

  await loaders.loadFullApp()
}
