declare module 'react-reconciler' {
  interface ReconcilerRoot {
    readonly _attoUiRootBrand?: unique symbol
  }

  interface ReconcilerInstance {
    createContainer(...args: readonly unknown[]): ReconcilerRoot
    updateContainer(
      element: unknown,
      container: ReconcilerRoot,
      parentComponent: unknown,
      callback: null | (() => void),
    ): void
  }

  function createReconciler(hostConfig: Record<string, unknown>): ReconcilerInstance

  export = createReconciler
}

declare module 'react-reconciler/constants' {
  export const DefaultEventPriority: number
  export const LegacyRoot: number
}
