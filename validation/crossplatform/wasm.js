// Métricas wasm32 vía node — bits IEEE-754 exactos. argv[2] = ruta al pkg.
const sw = require(process.argv[2]);
const h = x => { const b = new ArrayBuffer(8); new DataView(b).setFloat64(0, x, true);
  return Buffer.from(b).toString('hex'); };
const out = {};
let m = new sw.Sir(100, 0.08, 0.1, 10, 42); m.step(500);
out.sir = { recovered: h(m.recovered), infected: h(m.infected) };
let s = new sw.Schelling(50, 0.85, 0.375, 42); s.step(200);
out.schelling = { happy: h(s.happy), similarity: h(s.mean_similarity) };
let g = new sw.Sugarscape(50, 400, 1, 42); g.step(200);
out.sugarscape = { population: g.population, gini: h(g.gini) };
console.log(JSON.stringify(out));
