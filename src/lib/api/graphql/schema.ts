import {
  type DocumentIR,
  type FragmentDef,
  type SchemaIR,
  type Selection,
  type TypeDef,
  type TypeRef,
  type ValueNode,
} from './parser'

export const BUILTIN_SCALARS = new Set(['Int', 'Float', 'String', 'Boolean', 'ID'])

export interface ValidationError {
  path: string
  message: string
}

export function mergeSchemas(base: SchemaIR, extension: SchemaIR): { schema: SchemaIR; errors: ValidationError[] } {
  const errors: ValidationError[] = []
  const types = new Map(base.types)
  const rootNames = new Set([extension.queryRoot, extension.mutationRoot, extension.subscriptionRoot])
  for (const [name, def] of extension.types) {
    if (rootNames.has(name)) continue
    const existing = types.get(name)
    if (!existing) {
      types.set(name, def)
      continue
    }
    if (existing.kind !== def.kind) {
      errors.push({ path: name, message: `type '${name}' is ${existing.kind} in the local schema but ${def.kind} in the extension schema` })
      continue
    }
    mergeTypeFields(existing, def, errors)
  }
  const queryRootDef = types.get(base.queryRoot)
  const extQueryDef = extension.types.get(extension.queryRoot)
  const mutationRootDef = types.get(base.mutationRoot)
  const extMutationDef = extension.types.get(extension.mutationRoot)
  if (queryRootDef && extQueryDef) mergeRootFields(queryRootDef, extQueryDef, errors)
  if (mutationRootDef && extMutationDef) mergeRootFields(mutationRootDef, extMutationDef, errors)
  const schema: SchemaIR = {
    types,
    queryRoot: base.queryRoot,
    mutationRoot: base.mutationRoot,
    subscriptionRoot: base.subscriptionRoot,
  }
  return { schema, errors }
}

function mergeTypeFields(local: TypeDef, ext: TypeDef, errors: ValidationError[]) {
  const existing = new Set(local.fields.map((f) => f.name))
  for (const field of ext.fields) {
    if (existing.has(field.name)) {
      errors.push({
        path: `${local.name}.${field.name}`,
        message: `field '${local.name}.${field.name}' is defined by both schemas — remove it from the extension schema`,
      })
      continue
    }
    local.fields.push(field)
  }
  const values = new Set(local.enumValues)
  for (const v of ext.enumValues) {
    if (!values.has(v)) local.enumValues.push(v)
  }
}

function mergeRootFields(root: TypeDef, ext: TypeDef, errors: ValidationError[]) {
  mergeTypeFields(root, ext, errors)
}

export function isComposite(def: TypeDef | undefined): def is TypeDef {
  return !!def && (def.kind === 'object' || def.kind === 'interface' || def.kind === 'union')
}

export function typeName(ref: TypeRef): string {
  return ref.ofType === 'named' ? ref.name : typeName(ref.inner)
}

export function typeExists(schema: SchemaIR, name: string): boolean {
  return BUILTIN_SCALARS.has(name) || schema.types.has(name)
}

interface ValidateCtx {
  schema: SchemaIR
  fragments: Map<string, FragmentDef>
  declaredVars: Set<string>
  varTypes: Map<string, import('./parser').TypeRef>
  errors: ValidationError[]
  validatedFragments: Set<string>
}

function* op_variables(doc: DocumentIR): Generator<[string, import('./parser').TypeRef]> {
  for (const op of doc.operations) {
    for (const v of op.variables) yield [v.name, v.type]
  }
}

export function validateDocument(doc: DocumentIR, schema: SchemaIR, docKey: string): ValidationError[] {
  const errors: ValidationError[] = []
  const ctx: ValidateCtx = {
    schema,
    fragments: new Map(doc.fragments.map((f) => [f.name, f])),
    declaredVars: new Set(),
    varTypes: new Map(op_variables(doc)),
    errors,
    validatedFragments: new Set(),
  }

  for (const op of doc.operations) {
    const rootName =
      op.type === 'query' ? schema.queryRoot : op.type === 'mutation' ? schema.mutationRoot : schema.subscriptionRoot
    const root = schema.types.get(rootName)
    if (!root) {
      errors.push({ path: docKey, message: `schema has no ${op.type} root` })
      continue
    }
    for (const v of op.variables) {
      if (!typeExists(schema, typeName(v.type))) {
        errors.push({ path: docKey, message: `unknown type '${typeName(v.type)}' for variable '$${v.name}'` })
      }
      ctx.declaredVars.add(v.name)
    }
    validateSelections(op.selections, root, `${docKey}:${op.name ?? op.type}`, ctx)
  }

  for (const fragment of doc.fragments) {
    validateFragment(fragment, docKey, ctx)
  }
  return errors
}

function collectVariables(value: ValueNode, out: Set<string>) {
  if (value.kind === 'variable') out.add(value.name)
  else if (value.kind === 'list') value.values.forEach((v) => collectVariables(v, out))
  else if (value.kind === 'object') value.fields.forEach((f) => collectVariables(f.value, out))
}

function validateSelections(selections: Selection[], parent: TypeDef, path: string, ctx: ValidateCtx) {
  if (selections.length === 0) {
    ctx.errors.push({ path, message: `selection set on '${parent.name}' is empty` })
    return
  }
  for (const sel of selections) {
    if (sel.kind === 'field') {
      if (parent.kind === 'union' && sel.name !== '__typename') {
        ctx.errors.push({
          path,
          message: `union '${parent.name}' only allows __typename and fragment spreads, got field '${sel.name}'`,
        })
        continue
      }
      const fieldPath = `${path}.${sel.alias ?? sel.name}`
      const field = parent.fields.find((f) => f.name === sel.name)
      if (!field) {
        ctx.errors.push({ path: fieldPath, message: `unknown field '${sel.name}' on type '${parent.name}'` })
        continue
      }
      const fieldArgs = new Set(field.args.map((a) => a.name))
      for (const arg of sel.args) {
        const srvArg = field.args.find((a) => a.name === arg.name)
        if (!fieldArgs.has(arg.name)) {
          ctx.errors.push({ path: fieldPath, message: `unknown argument '${arg.name}' on field '${parent.name}.${sel.name}'` })
        }
        const vars = new Set<string>()
        collectVariables(arg.value, vars)
        for (const v of vars) {
          if (!ctx.declaredVars.has(v)) {
            ctx.errors.push({ path: fieldPath, message: `variable '$${v}' is used but not declared` })
          }
          if (srvArg && srvArg.type.nonNull && !srvArg.hasDefault) {
            const vt = ctx.varTypes.get(v)
            if (vt && !vt.nonNull) {
              ctx.errors.push({
                path: fieldPath,
                message: `variable '$${v}' is nullable but argument '${arg.name}' of '${parent.name}.${sel.name}' requires a non-null value`,
              })
            }
          }
        }
      }
      for (const arg of field.args) {
        if (fieldArgs.has(arg.name)) continue
        if (arg.type.nonNull && !arg.hasDefault) {
          ctx.errors.push({
            path: fieldPath,
            message: `required argument '${arg.name}' on field '${parent.name}.${sel.name}' is not provided`,
          })
        }
      }
      const fieldTypeName = typeName(field.type)
      const fieldTypeDef = ctx.schema.types.get(fieldTypeName)
      if (!typeExists(ctx.schema, fieldTypeName)) {
        ctx.errors.push({ path: fieldPath, message: `unknown type '${fieldTypeName}' (field '${parent.name}.${sel.name}')` })
        continue
      }
      if (isComposite(fieldTypeDef)) {
        validateSelections(sel.selections, fieldTypeDef, fieldPath, ctx)
      } else if (sel.selections.length > 0) {
        ctx.errors.push({ path: fieldPath, message: `scalar/enum field '${parent.name}.${sel.name}' cannot have a selection set` })
      }
    } else if (sel.kind === 'inline') {
      if (!sel.typeCondition) {
        validateSelections(sel.selections, parent, `${path}@inline`, ctx)
        continue
      }
      const def = ctx.schema.types.get(sel.typeCondition)
      if (!def) {
        ctx.errors.push({ path, message: `unknown type '${sel.typeCondition}' in inline fragment` })
        continue
      }
      if (!isComposite(def)) {
        ctx.errors.push({ path, message: `inline fragment condition '${sel.typeCondition}' must be a composite type` })
        continue
      }
      validateSelections(sel.selections, def, `${path} on ${sel.typeCondition}`, ctx)
    } else {
      const fragment = ctx.fragments.get(sel.name)
      if (!fragment) {
        ctx.errors.push({ path, message: `unknown fragment '${sel.name}'` })
        continue
      }
      validateFragment(fragment, path, ctx)
    }
  }
}

function validateFragment(fragment: FragmentDef, path: string, ctx: ValidateCtx) {
  if (ctx.validatedFragments.has(fragment.name)) return
  ctx.validatedFragments.add(fragment.name)
  const def = ctx.schema.types.get(fragment.typeCondition)
  if (!def) {
    ctx.errors.push({ path, message: `unknown type '${fragment.typeCondition}' in fragment '${fragment.name}'` })
    return
  }
  if (!isComposite(def)) {
    ctx.errors.push({ path, message: `fragment '${fragment.name}' condition '${fragment.typeCondition}' must be a composite type` })
    return
  }
  validateSelections(fragment.selections, def, `${path} ·${fragment.name}`, ctx)
}
