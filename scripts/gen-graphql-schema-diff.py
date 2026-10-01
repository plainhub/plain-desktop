from pathlib import Path
import re
from html import escape
from datetime import datetime
from zoneinfo import ZoneInfo

ROOT = Path(__file__).resolve().parents[1]
SOURCES = {
    'plain-app': ROOT.parent.joinpath('plain-app/shared/apitest/schema.graphqls'),
    'plain-desktop': ROOT / 'schema/schema.graphql',
}

def parse_sdl(path):
    src = path.read_text()
    src = re.sub(r'"""[\s\S]*?"""', '\n', src)
    src = re.sub(r'(?m)^\s*#[^\n]*', '', src)
    defs = {}
    pat = re.compile(r'(?m)^(type|input|enum|interface|union|scalar)\s+(\w+)\b[^\n{]*')
    for m in pat.finditer(src):
        kind, name = m.group(1), m.group(2)
        if name.startswith('__'):
            continue
        if kind == 'union':
            declaration = src[m.start():src.find('\n', m.start())]
            members = sorted(x for x in re.findall(r'\b[A-Za-z_]\w*\b', declaration.split('=', 1)[-1]) if x != name)
            key = '@Query' if name in ('QueryRoot', 'Query') else '@Mutation' if name in ('MutationRoot', 'Mutation') else name
            defs[key] = (kind, name, {'members': ' | '.join(members)})
            continue
        open_pos = src.find('{', m.end())
        if kind == 'scalar' or open_pos < 0:
            body = ''
            end = m.end()
        else:
            depth = 0
            in_string = False
            escaped = False
            end = open_pos
            for end in range(open_pos, len(src)):
                c = src[end]
                if in_string:
                    if escaped: escaped = False
                    elif c == '\\': escaped = True
                    elif c == '"': in_string = False
                    continue
                if c == '"': in_string = True
                elif c == '{': depth += 1
                elif c == '}':
                    depth -= 1
                    if depth == 0: break
            body = src[open_pos + 1:end]
        key = name
        if name in ('QueryRoot', 'Query'): key = '@Query'
        elif name in ('MutationRoot', 'Mutation'): key = '@Mutation'
        fields = {}
        if kind in ('type', 'input', 'interface'):
            lines = []
            buf = ''
            depth = 0
            for line in body.splitlines():
                line = line.strip()
                if not line: continue
                buf = (buf + ' ' + line).strip()
                depth += line.count('(') - line.count(')')
                if depth <= 0:
                    lines.append(buf); buf = ''
            for line in lines:
                line = re.sub(r'\s+', ' ', line).strip()
                fm = re.match(r'(\w+)\s*(\(.*?\))?\s*:\s*(.+)$', line)
                if fm:
                    fields[fm.group(1)] = (fm.group(2) or '') + ':' + fm.group(3).strip()
        elif kind == 'enum':
            fields = {x: x for x in re.findall(r'(?m)^\s*(\w+)\s*$', body) if not x.startswith('__')}
        elif kind == 'union':
            fields = {'members': re.sub(r'\s+', '', body)}
        else:
            fields = {}
        defs[key] = (kind, name, fields)
    return defs

schemas = {name: parse_sdl(path) for name, path in SOURCES.items()}
app, desktop = schemas['plain-app'], schemas['plain-desktop']
rows = []
for key in sorted(set(app) | set(desktop)):
    a, d = app.get(key), desktop.get(key)
    shown = key[1:] if key.startswith('@') else key
    if a is None:
        rows.append(('desktop独有类型', shown, 'plain-app 缺少', f'{d[0]}: {", ".join(d[2])}'))
        continue
    if d is None:
        rows.append(('plain-app 独有类型', shown, f'{a[0]}: {", ".join(a[2])}', 'desktop 缺少'))
        continue
    if a[0] != d[0]:
        rows.append(('字段签名不同', shown, a[0], d[0]))
        continue
    for field in sorted(set(a[2]) | set(d[2])):
        av, dv = a[2].get(field), d[2].get(field)
        if av is None:
            rows.append(('desktop额外字段/枚举值', f'{shown}.{field}', 'plain-app 缺少', dv))
        elif dv is None:
            rows.append(('plain-app 独有字段/枚举值', f'{shown}.{field}', av, 'desktop 缺少'))
        elif av != dv:
            rows.append(('字段签名不同', f'{shown}.{field}', av, dv))

# Keep stable grouping and row numbers for a reviewable static report.
kind_order = ['plain-app 独有类型', 'plain-app 独有字段/枚举值', '字段签名不同', 'desktop独有类型', 'desktop额外字段/枚举值']
rows.sort(key=lambda r: (kind_order.index(r[0]), r[1].lower()))

def cell(s):
    return '<code>' + escape(str(s)) + '</code>'
body = ''.join('<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>'.format(i, escape(kind), escape(ident), cell(left), cell(right)) for i,(kind,ident,left,right) in enumerate(rows,1))
counts = {}
for kind, *_ in rows: counts[kind] = counts.get(kind,0)+1
summary = '；'.join(f'{k} {counts[k]} 项' for k in kind_order if counts.get(k))
app_only_mutations = set(app['@Mutation'][2]) - set(desktop['@Mutation'][2])
app_only_queries = set(app['@Query'][2]) - set(desktop['@Query'][2])
desktop_only_mutations = set(desktop['@Mutation'][2]) - set(app['@Mutation'][2])
desktop_only_queries = set(desktop['@Query'][2]) - set(app['@Query'][2])
html = f'''<!doctype html><html lang="zh-CN"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>plain-app 与 plain-desktop GraphQL Schema 差异</title><style>*{{box-sizing:border-box}}body{{margin:0;background:#f4f6fa;color:#192230;font:14px/1.6 system-ui,-apple-system,sans-serif}}main{{max-width:1480px;margin:32px auto;padding:0 24px}}h1{{font-size:28px}}.muted{{color:#667085}}.box{{background:#fff;border:1px solid #dfe4ec;border-radius:12px;padding:18px;margin:16px 0}}.controls{{display:flex;gap:12px}}input,select{{font:inherit;padding:9px;border:1px solid #ccd3dd;border-radius:8px}}input{{flex:1}}.tablewrap{{overflow:auto;margin-top:12px}}table{{width:100%;border-collapse:collapse;background:#fff}}th,td{{text-align:left;border-bottom:1px solid #e7eaf0;padding:9px;vertical-align:top;overflow-wrap:anywhere}}th{{position:sticky;top:0;background:#edf1f6}}td:first-child{{text-align:right;width:48px;color:#3156a4;font-weight:bold}}tr[hidden]{{display:none}}</style><main><h1>plain-app 与 plain-desktop GraphQL Schema 差异</h1><p class="muted">更新于 {datetime.now(ZoneInfo('Asia/Shanghai')).strftime('%Y-%m-%d')} · 主 schema SDL 结构化对比 · plain-app shared/apitest/schema.graphqls vs plain-desktop schema/schema.graphql</p><section class="box"><b>比较范围</b><p>比较字段参数、返回类型、空值标记与枚举值。将 plain-app 的 Query/Mutation 与 plain-rs 的 QueryRoot/MutationRoot 映射到同一操作根；忽略 introspection 元素、描述文本和 SDL 声明顺序。不同字段形状不等同于 resolver 行为差异。</p><p>总计 {len(rows)} 项：{escape(summary)}。</p><p>操作根差异：plain-app 独有 Mutation {len(app_only_mutations)} 项、Query {len(app_only_queries)} 项；plain-desktop 独有 Mutation {len(desktop_only_mutations)} 项、Query {len(desktop_only_queries)} 项。</p><p class="muted">peer/guest SDL 不在比较范围。此报告描述 schema 形状，不代表对应字段的后端行为已一致。</p></section><div class="controls"><input id="q" placeholder="搜索类型、字段或签名…"><select id="kind"><option value="">所有差异</option>{''.join(f'<option>{escape(k)}</option>' for k in kind_order)}</select></div><p class="muted" id="count"></p><div class="tablewrap"><table><thead><tr><th>#</th><th>差异类别</th><th>类型 / 字段</th><th>plain-app</th><th>plain-desktop</th></tr></thead><tbody>{body}</tbody></table></div></main><script>const q=document.querySelector('#q'),k=document.querySelector('#kind'),rs=[...document.querySelectorAll('tbody tr')],c=document.querySelector('#count');function run(){{let n=0;for(const r of rs){{const ok=(!q.value||r.innerText.toLowerCase().includes(q.value.toLowerCase()))&&(!k.value||r.cells[1].innerText===k.value);r.hidden=!ok;if(ok)n++}}c.textContent=`显示 ${{n}} / ${{rs.length}} 项`}}[q,k].forEach(x=>x.addEventListener('input',run));run()</script></html>'''
Path(ROOT / 'docs/graphql-schema-diff.html').write_text(html)
print(f'Generated {len(rows)} semantic differences: {summary}')
