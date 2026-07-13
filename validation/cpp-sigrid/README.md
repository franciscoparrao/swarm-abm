# SIGRID en C++ — port de producción, validado contra swarm-abm (oráculo)

Implementación en C++ del modelo de ovejas SIGRID, para la vía de escala
(OpenMP → BSPonMPI) que pide el objetivo del proyecto. **swarm-abm es el oráculo
de referencia**: el C++ se valida contra él por paridad distribucional. Ver el
plan completo en `../../docs/PLAN_PORT_CPP_SIGRID.md`.

## Estado: Hito 1 completo — subconjunto de *screening* (oveja + zorro)

`sheep_fox.cpp` porta el subconjunto de screening del SIGRID committeado
(`models/sigrid/src/lib.rs @ HEAD`): ovejas (adultas/corderos, con miedo, huida,
forrajeo con cohesión y evitación de riesgo, maduración, energía) y zorros culpeo
(curva de actividad horaria, hambre, detección, selección de presa por
vulnerabilidad, predación con sus modificadores de cobertura/grupo/condición/
defensa materna). Con los parámetros de screening no hay perros, liebres ni
chillas, así que esas ramas se omiten (Hitos 2+).

- **Determinista**: sembrado (`mt19937_64`); dos corridas idénticas → mismo hash.
  A diferencia de `sim2-agricultores` (que no es reproducible), este port sí lo
  es desde el inicio.
- **Semántica del motor replicada**: índice espacial = instantánea de inicio de
  paso; activación aleatoria (orden barajado por paso); mutaciones sobre el
  arreglo vivo.

### Validación contra el oráculo (paridad distribucional)

Como el RNG difiere (ChaCha8 en swarm-abm vs mt19937 en C++), la paridad es
distribucional, no bit-exacta — **misma metodología que la paridad vs Mesa**
(`models/sigrid/PARITY.md`). Barrido `sheep_density × fox_eff`, 14 días, 3
semillas/punto, 9 puntos, con `fox_density` en su default en ambos:

| sheep_dens | fox_eff | C++ | swarm-abm | \|Δ\| |
|---:|---:|---:|---:|---:|
| 0.96 | 0.08 | 42.5% | 43.5% | 1.0 |
| 0.96 | 0.14 | 52.3% | 50.1% | 2.2 |
| 0.96 | 0.26 | 60.8% | 58.2% | 2.6 |
| 2.00 | 0.08 | 24.7% | 23.8% | 0.9 |
| 2.00 | 0.14 | 28.2% | 27.8% | 0.4 |
| 2.00 | 0.26 | 31.2% | 30.3% | 0.9 |
| 4.00 | 0.08 | 12.9% | 12.9% | 0.0 |
| 4.00 | 0.14 | 14.9% | 14.3% | 0.7 |
| 4.00 | 0.26 | 17.0% | 16.5% | 0.5 |

**Pearson r = 0.9986 · Spearman ρ = 1.0000 · RMSE = 1.30 pp · sesgo +0.80 pp.**
La paridad C++↔swarm-abm **supera** la de swarm-abm↔Mesa (Pearson 0.966): el
port reproduce el comportamiento del oráculo en todo el rango de loss (13–61%).

### Hallazgo colateral de la validación

El barrido destapó que el CLI del oráculo (`models/sigrid/src/main.rs @ HEAD`)
**documenta `--fox-density` en su ayuda pero no lo parsea** — el argumento se
ignora y `fox_density` queda fijo en 8.4. (Este port C++ sí lo aplica.) Es
justamente el tipo de discrepancia que el esquema de dos motores está pensado
para cazar. Pendiente de arreglar en el `main.rs` de swarm-abm.

## Reproducir

```bash
cd validation/cpp-sigrid
g++ -std=c++17 -O3 -march=native -flto sheep_fox.cpp -o sheep_fox
./sheep_fox --days 30 --seed 1000 --seeds 5          # corrida directa
python3 parity.py <ruta-al-binario-sigrid-de-swarm-abm>   # barrido de paridad
```

El oráculo se construye desde un árbol limpio en HEAD:
`cargo build --release -p sigrid --bin sigrid`.

## Próximos hitos

2. **+ Perros guardianes** (la intervención): disuasión, memoria de peligro,
   acecho/intercepción. Re-validar contra swarm-abm con `--dogs`.
3. **+ Liebres y chillas** (presa alternativa, segundo depredador). Modelo
   completo de screening.
4. **OpenMP** (un nodo): mismo resultado validado, con speedup. Aquí el oráculo
   se vuelve indispensable — caza bugs de concurrencia.
5. **(Según decisión de alcance)** subsistemas del Mesa completo (infraestructura,
   estacionalidad, rasters GIS) — ver §7 del plan. Se agregan primero al oráculo.
