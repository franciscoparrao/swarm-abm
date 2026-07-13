# Arnés de reproducibilidad — sim2-agricultores (C++)

Demuestra que el simulador C++ [`ManachoM/sim2-agricultores`](https://github.com/ManachoM/sim2-agricultores)
no es reproducible: dos corridas con configuración idéntica producen resultados
distintos. Ver el análisis completo en `../VS_CPP_AGROSIM.md`.

- `Dockerfile.repro` — imagen autocontenida (Ubuntu 24.04 + g++ 13 + libpqxx +
  PostgreSQL 16). Compila `agro-sim` desde el repo C++ (elimina los headers
  vendored de pqxx para usar los del sistema; desactiva `-Werror` implícito).
- `run_demo.sh` — arranca Postgres, crea la BD que el sim espera
  (`postgres:secret@localhost:5432/sim-db`), corre el binario dos veces con la
  misma config y compara el vector de resultados agregados vía hash MD5.

## Uso

```bash
git clone https://github.com/ManachoM/sim2-agricultores
cp Dockerfile.repro run_demo.sh sim2-agricultores/
cd sim2-agricultores
docker build -f Dockerfile.repro -t agro-sim-repro .
docker run --rm -v "$PWD/run_demo.sh:/run_demo.sh:ro" agro-sim-repro bash /run_demo.sh
```

Resultado observado (2026-07-13, config chica): las dos corridas escriben 1260
filas cada una, con hashes MD5 distintos y 65 filas con valores diferentes.
Causa: el código siembra cada RNG con `std::random_device` (sin semilla fija).
