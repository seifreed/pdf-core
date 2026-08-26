# Veredicto

## Estado de seguimiento (2026-08-26)

Este documento conserva la auditoria original. Desde entonces se han cerrado
varios puntos: el workspace incluye todos los crates, `main` esta protegida,
las alertas de Dependabot estan resueltas, el parser tiene lecturas acotadas y
validacion de limites en las rutas de xref/object streams, y la serializacion
restaura el estado documental lossless disponible. Tambien existe un corpus
reproducible de 2.806 fixtures upstream mas tres regresiones, metricas
diferenciales, una campana fuzz local de 16.000 ejecuciones sin crashes y un
chequeo completo de 205 mapeos contra veraPDF 1.30.2.

Actualizacion 2026-08-27: la campana de ClusterFuzzLite encontro y ya tiene
corregidos dos crashes reproducibles en entradas malformadas, uno en hex
invalido de CMap y otro en cabeceras JBIG2 truncadas. Las correcciones estan
en `9b640bb` y `44dd805`, con regresiones locales y fuzz dirigido en verde.
La matriz de CI tambien instala qpdf/MuPDF para el diferencial, conserva el
log y las metricas de memoria como artefacto, y ejecuta el corpus externo
completo. Los workflows de CI y ClusterFuzzLite sobre `44dd805` siguen en
curso; hasta que terminen no se debe contar esta evidencia como verde.

Siguen pendientes para cerrar este roadmap: evidencia verde y repetida en CI
para corpus/differential/fuzzing y bindings; semantica estricta completa y
presupuesto de recursos aplicado a todas las operaciones; matriz normativa
clausula por clausula y validacion PDF/A/PDF/UA no experimental; cobertura
completa de CMap, fuentes, herencia, revisiones hibridas y codecs; API/visitors
estables y reorganizacion final de crates; y publicacion firmada en crates.io,
PyPI y npm con consumidores externos. El chequeo local de `cargo-semver-checks`
pasa para `pdf-ast` contra `0.1.0`; el workspace completo no es auditable
porque `pdf-ast-dynamic-plugin-example` no esta publicado en crates.io.

**`pdf-core` es actualmente una alpha técnica avanzada, no una beta y todavía no una librería preparada para procesar PDFs no confiables en producción.**

Mi valoración aproximada sería:

* **Núcleo parser/AST:** 5,5–6/10.
* **Repositorio completo como producto:** 4–4,5/10.
* **Madurez de release:** apto para una **`0.1.1-alpha` experimental**.
* **Madurez de seguridad:** todavía insuficiente para usarlo como motor de análisis de adjuntos procedentes de Internet sin aislamiento adicional.
* **Distancia a una beta creíble:** dos o tres hitos importantes.
* **Distancia a una 1.0:** considerable, principalmente por falta de pruebas de conformidad, fuzzing continuo, estabilización de API y eliminación de implementaciones provisionales.

No es un simple esqueleto. Hay bastante implementación real: parser de xref clásico y xref streams, cadenas `/Prev`, actualizaciones incrementales, object streams, recuperación tolerante, árbol de páginas, AST rico, formularios, acciones, XMP, XFA, contenido multimedia, firmas y cifrado. Esa base tiene valor y puede convertirse en algo muy potente para análisis estructural, forense y de seguridad de PDF.

## Evaluación por áreas

| Área                        |       Nota | Lectura                                                          |
| --------------------------- | ---------: | ---------------------------------------------------------------- |
| Arquitectura y visión       |   **7/10** | Buena separación conceptual y AST muy ambicioso                  |
| Cobertura sintáctica de PDF |   **6/10** | Bastante amplia, pero con casos límite importantes               |
| Corrección y conformidad    | **3,5/10** | Falta evidencia contra corpus normativos y parsers de referencia |
| Resistencia a PDFs hostiles | **3,5/10** | Hay límites, pero no están conectados de extremo a extremo       |
| Tests                       |   **5/10** | Muchos tests unitarios, poca validación externa real             |
| Análisis de seguridad       | **4,5/10** | Buen planteamiento, todavía heurístico                           |
| API y esquema AST           |   **3/10** | Amplios, pero no estabilizados                                   |
| Bindings y distribución     | **2,5/10** | Existen estructuras, no una cadena de entrega sólida             |
| Documentación y gobernanza  |   **3/10** | Hay documentación, pero promete más de lo implementado           |

---

# Lo mejor del proyecto

## 1. El núcleo tiene una arquitectura con sentido

La idea de representar el PDF como un grafo semántico, en lugar de limitarse a devolver diccionarios y objetos, es útil para:

* malware analysis;
* análisis de revisiones incrementales;
* búsqueda de JavaScript y acciones;
* firmas y validación documental;
* comparación estructural;
* extracción de relaciones;
* generación de reglas y consultas.

El modelo incluye tipos específicos para catálogo, páginas, recursos, fuentes, XObjects, acciones, formularios, firmas, cifrado, árboles de nombres, estructura accesible, multimedia, contenido 3D, output intents y elementos sospechosos. Esto es una diferenciación real frente a un parser mínimo.

## 2. El parser ya maneja componentes difíciles

No te has quedado en leer objetos directos. El código contempla:

* xref tables y xref streams;
* PDFs híbridos mediante `/XRefStm`;
* cadenas de revisiones incrementales mediante `/Prev`;
* object streams;
* recuperación de xref mediante escaneo;
* documentos linealizados;
* page tree;
* formularios AcroForm y XFA;
* anotaciones y acciones;
* metadatos XMP;
* DSS/LTV;
* RichMedia, audio, vídeo y 3D.

Es una base considerable para una primera versión experimental.

## 3. Has incorporado medidas de defensa desde el diseño

`PerformanceLimits` contempla tamaño de fichero, tamaño de objeto, ratio de descompresión, profundidad, memoria, número de nodos y aristas, concurrencia y tiempo máximo. También existen límites específicos durante la decodificación de streams. Esto es una muy buena dirección para un parser de un formato históricamente expuesto a ataques de consumo de recursos.

## 4. La CI básica existe y funciona

El workflow principal ejecuta formato, Clippy, tests, cobertura, compilación multiplataforma y auditoría de dependencias. La ejecución principal que pude verificar terminó correctamente el **8 de febrero de 2026**, incluyendo tests y `cargo audit`; Clippy terminó con éxito, aunque generó anotaciones y no se ejecuta con warnings convertidos en errores.

---

# Los bloqueantes de madurez

## 1. La documentación promete bastante más de lo que el código entrega

Este es ahora mismo el principal problema de credibilidad.

El README utiliza expresiones como cobertura completa de PDF 2.0 y componentes “production ready”. Sin embargo:

* El decoder JBIG2 contiene símbolos de 8×8 de relleno, posicionamiento simplificado, regiones sin aplicar y código expresamente descrito como simplificado.
* JPX no decodifica realmente JPEG 2000 a píxeles; extrae o concatena codestreams.
* `pdf-text-intel` introduce literalmente texto y coordenadas de ejemplo.
* `pdf-diff` compara páginas utilizando cadenas ficticias del tipo `Sample text from page`.
* El supuesto streaming parser corta el fichero en chunks arbitrarios y reconoce que su implementación es simplificada.

Esto debe corregirse antes de promocionar el proyecto: o completas esas funciones o las marcas como **experimental, parcial o no implementadas**.

Mi recomendación inmediata es publicar una matriz como esta:

| Función                                | Estado                                               |
| -------------------------------------- | ---------------------------------------------------- |
| Objetos básicos, arrays y diccionarios | Implementado                                         |
| xref clásico                           | Implementado                                         |
| xref stream                            | Implementado, pendiente de corpus amplio             |
| Object streams                         | Parcial                                              |
| Actualizaciones incrementales          | Parcial/experimental                                 |
| Flate, ASCII85, LZW, RunLength         | Implementado                                         |
| CCITT                                  | Experimental                                         |
| JBIG2                                  | Placeholder, no soportado                            |
| JPX                                    | Inspección de contenedor, no decodificación completa |
| Extracción de texto                    | Experimental                                         |
| PDF/A                                  | Validación parcial, no certificable                  |
| PDF/UA                                 | Validación parcial, no certificable                  |
| Streaming                              | Prototipo                                            |
| Python/JavaScript                      | Experimental                                         |

## 2. El parser todavía tiene rutas que pueden producir resultados engañosos

Hay varios ejemplos importantes:

* `PdfParser` expone `max_depth` y `max_errors`, pero esos campos no se transmiten realmente al `DocumentParser`; el parser recibe esencialmente tolerancia y `PerformanceLimits`.
* `parse_objects` detiene el análisis en el primer error y devuelve `Ok` con los objetos parciales.
* `parse_object` es actualmente un alias de `parse_value`, no un parser independiente de objetos indirectos.
* En varias situaciones, un objeto inexistente, truncado o que no coincide con su `ObjectId` acaba convertido en `PdfValue::Null`.
* Esto hace que el modo denominado “strict” no tenga todavía una semántica completamente estricta.

Necesitas tres modos perfectamente definidos:

### `Strict`

Cualquier violación estructural relevante devuelve error. Nunca sustituye silenciosamente objetos por `Null`.

### `Tolerant`

Recupera cuando es posible, pero devuelve diagnósticos estructurados:

```text
object_id
offset
error_code
recovery_action
confidence
bytes_consumed
```

### `Forensic`

Conserva simultáneamente:

* estructura declarada por xref;
* estructura recuperada por escaneo;
* objetos duplicados;
* contenido sobrescrito entre revisiones;
* offsets exactos;
* bytes residuales;
* discrepancias entre `/Length` y delimitadores.

Ese tercer modo podría convertirse en una de las grandes diferencias de `pdf-core`.

## 3. Hay problemas concretos de robustez que deben ser P0

### Lectura fija de objetos a 64 KiB

Los objetos ordinarios se cargan leyendo un buffer fijo de 65.536 bytes. Un objeto indirecto válido mayor que eso puede quedar truncado y terminar como `Null`, especialmente streams grandes o diccionarios con contenido extenso.

Debes sustituirlo por un lector acotado que determine los límites mediante:

1. offset del siguiente objeto en xref;
2. `/Length` directo o indirecto;
3. delimitadores válidos;
4. un máximo configurable.

### Slices sin validación suficiente en object streams

El parser utiliza rangos como `data[..first]` y `data[*obj_offset..]`. En un object stream malformado, valores de `/First` u offsets fuera del buffer pueden provocar panic. Debes validar todos los rangos con `get()`, aritmética comprobada y errores específicos.

### `/Length` indirecto no resuelto durante el parsing

Cuando `/Length` es una referencia indirecta, el parser busca el primer `endstream` como fallback. Esto es peligroso para corrección porque la secuencia puede existir dentro de los datos comprimidos. Cuando no encuentra `endstream`, puede tratar todo el resto del buffer como stream. Además, la conversión de una longitud PDF negativa a `usize` debería rechazarse explícitamente.

La solución correcta es separar:

* parsing inicial del diccionario;
* resolución de `/Length`;
* lectura exacta del stream;
* recuperación heurística solo en modo tolerante o forense.

### Código de recuperación con valores artificiales

Existe una ruta heredada de parsing de xref streams que puede crear object IDs y offsets codificados de forma fija para “satisfacer” PDFs de prueba. Aunque aparentemente no es la ruta principal moderna, no debe permanecer en código de producción. Una recuperación puede ser aproximada, pero nunca debe inventar silenciosamente offsets concretos.

## 4. Los límites de seguridad no están conectados de extremo a extremo

Tienes una buena definición de límites, pero no toda la ruta principal los aplica:

* el guard de nodos, aristas, memoria y timeout no está integrado en cada creación o resolución;
* determinados fallos terminan en `Null`, dificultando distinguir un límite alcanzado de un objeto realmente nulo;
* el análisis de seguridad utiliza `stream.decode()` y posteriormente recorta a 1 MiB, pero la descompresión completa puede haberse producido antes;
* `decode_stream()` sin límites utiliza `usize::MAX`.

Por tanto, un consumidor puede creer que está protegido por `PerformanceLimits` cuando ciertas rutas siguen siendo prácticamente ilimitadas.

La regla debería ser:

> Ninguna API que decodifique, resuelva, recorra o serialice contenido no confiable puede funcionar sin un `ResourceBudget`.

Yo introduciría un objeto compartido:

```rust
pub struct ResourceBudget {
    pub max_input_bytes: u64,
    pub max_decoded_bytes_total: u64,
    pub max_decoded_bytes_per_stream: u64,
    pub max_decode_ratio: u64,
    pub max_objects: usize,
    pub max_nodes: usize,
    pub max_edges: usize,
    pub max_depth: usize,
    pub deadline: Option<Instant>,
    pub cancellation: CancellationToken,
}
```

Y todas las operaciones deberían consumir presupuesto del mismo contexto.

## 5. Tienes muchos tests, pero todavía poca evidencia

El repositorio incluye bastantes tests específicos de formularios, acciones, CCITT, cifrado, firmas, contenido y otros componentes. Eso es positivo. Sin embargo, los tests de corpus finalizan correctamente cuando no existe `pdfs/CORPUS.json`; por tanto, la CI puede aparecer verde sin haber procesado un solo PDF del corpus. Además, el test solo exige que los documentos se acepten en modo tolerante y que al menos algún stream pueda decodificarse.

`pdf-fuzz` tampoco es todavía fuzzing propiamente dicho: realiza mutaciones deterministas de bits y truncados sobre un número pequeño de ficheros, sin cobertura guiada, minimización, diccionario del formato, corpus de regresión ni integración continua especializada.

Para un parser PDF, necesitas cuatro capas:

1. Tests unitarios de primitivas.
2. Tests de integración con PDFs construidos para cada característica.
3. Corpus reales y corpus de conformidad.
4. Fuzzing continuo y differential testing.

El corpus de veraPDF proporciona casos documentados para PDF/A 1–4 y PDF/UA, mientras que los repositorios recopilados por PDF Association incluyen corpus como Isartor y otros conjuntos destinados a probar parsers y validadores. Son referencias adecuadas para transformar tus afirmaciones de conformidad en resultados medibles. ([GitHub][1]) ([GitHub][2]) ([GitHub][3])

Para fuzzing, ClusterFuzzLite soporta proyectos Rust y fuzzers basados en libFuzzer, ejecución en pull requests, campañas periódicas, cobertura y conservación de crash artifacts. Es el siguiente paso natural antes de plantear una incorporación a OSS-Fuzz. ([Google GitHub][4]) ([Google GitHub][5]) ([Google GitHub][6])

## 6. La conformidad PDF/A y PDF/UA no está madura

Actualmente tienes dos líneas distintas:

* validadores dentro de `src/validation`;
* el crate separado `pdf-compliance`.

El validador principal contiene más comprobaciones, pero sigue usando aproximaciones; por ejemplo, en ciertos CID fonts asume que la presencia de `DescendantFonts` es suficiente para considerarlos embebidos. El crate independiente expone muchos perfiles en el enum, pero las funciones implementadas se limitan fundamentalmente a PDF/A-1b y PDF/UA-1 con un conjunto reducido de reglas.

No deberías decir todavía “valida PDF/A” o “valida PDF/UA” sin calificadores. Debería presentarse como:

> “Experimental preflight checks for selected PDF/A and PDF/UA requirements.”

Para llamarlo validador necesitarás:

* identificador exacto de cada regla;
* referencia a cláusula normativa;
* perfil;
* objeto y offset implicados;
* severidad;
* resultado esperado en corpus positivo y negativo;
* comparación contra veraPDF;
* declaración de cobertura y reglas no implementadas.

## 7. La API y el esquema todavía no están congelados

La serialización declara un esquema AST `1.1`, mientras que los metadatos del grafo escriben una versión `1.0`. Durante la deserialización, la restauración completa del mapeo entre `ObjectId` y nodos aparece todavía pendiente. Además, algunos tipos se serializan convirtiendo nombres de enums a cadenas de depuración, lo cual no es una base estable para compatibilidad a largo plazo.

Antes de una beta necesitas decidir qué quieres preservar:

* identidad del objeto;
* generación;
* offset;
* longitud declarada y longitud observada;
* revisión documental;
* bytes originales;
* valor normalizado;
* estado de decodificación;
* error de parsing;
* acción de recuperación;
* relaciones semánticas.

Mi recomendación es separar dos capas:

### Lossless syntax model

Representa exactamente lo encontrado en el fichero, incluyendo offsets, tokens, objetos duplicados y datos corruptos.

### Semantic graph

Representa catálogo, páginas, acciones, scripts, fuentes, firmas y relaciones.

Ahora mismo ambas responsabilidades están demasiado mezcladas.

## 8. El repositorio necesita consolidación y gobernanza

Hay varias inconsistencias:

* `LICENSE` indica MIT.
* El `Cargo.toml` principal indica MIT.
* Algunos encabezados del código y documentación indican GPL-3.0.
* Subcrates y bindings utilizan `MIT OR Apache-2.0`.
* Parte de la metadata todavía apunta al antiguo repositorio o a `pdf-ast/pdf-ast`.
* El crate se llama `pdf-ast`, el repositorio `pdf-core` y quedan referencias a `PDF-AST`.
* No hay GitHub Releases publicadas.
* La rama principal aparece sin protección.
* Los crates auxiliares no están declarados como miembros de un workspace raíz, por lo que el `cargo test --workspace` del root no los incorpora automáticamente.
* Hay actualizaciones de Dependabot relacionadas con advisories que siguen abiertas, incluyendo actualizaciones de `rustls-webpki` y `quinn-proto`.

Esto no es cosmético. Afecta a:

* adopción;
* cumplimiento de licencias;
* publicación en crates.io/PyPI/npm;
* confianza en el proyecto;
* gestión de vulnerabilidades;
* capacidad de mantener compatibilidad.

---

# El posicionamiento que más sentido tiene

No intentaría que la primera versión fuera simultáneamente:

* parser PDF 2.0 completo;
* renderer;
* extractor de texto avanzado;
* validador PDF/A y PDF/UA;
* motor de diff;
* antivirus de PDF;
* toolkit criptográfico;
* librería multiplataforma con bindings;
* parser streaming.

Eso dispersa demasiado la ingeniería.

La propuesta más fuerte sería:

> **Un parser lossless de PDF en Rust, orientado a análisis estructural, forense y de seguridad, con AST consultable, reconstrucción de revisiones y límites estrictos para entradas no confiables.**

Es decir, algo parecido al papel de `pefile` para PE, pero aplicado a PDF y con un modelo de grafo bastante más rico.

La diferenciación debería centrarse en:

1. Preservación de la estructura original.
2. Tolerancia controlada a PDFs corruptos.
3. Historial de revisiones incrementales.
4. Detección de inconsistencias y polyglots.
5. Extracción de JavaScript, acciones y adjuntos.
6. Límites de recursos verificables.
7. API de consultas estructurales.
8. Serialización AST estable.

---

# Roadmap recomendado

## `v0.1.1-alpha` — Saneamiento y verdad documental

### Trabajo

* Elegir una única licencia y corregir todos los encabezados.
* Fijar el nombre canónico: `pdf-core` como proyecto y `pdf-ast` o `pdf-core` como crate.
* Actualizar todos los enlaces y metadatos.
* Sustituir “Full PDF 2.0” y “production ready” por una matriz de capacidades.
* Crear un workspace Cargo real.
* Incluir en CI todos los crates, features y bindings.
* Mover `pdf-diff` y `pdf-text-intel` a `experimental/` mientras contengan datos ficticios.
* Eliminar la ruta de xref con offsets y objetos artificiales.
* Añadir `SECURITY.md`, `CHANGELOG.md`, MSRV y política de compatibilidad.
* Resolver las actualizaciones de seguridad pendientes.
* Proteger `main` y exigir CI.

### Criterio de salida

Todos los miembros compilan y se prueban en CI; no existen placeholders dentro de funcionalidades anunciadas; la licencia y la identidad del proyecto son coherentes.

---

## `v0.2.0-alpha` — Parser correcto y endurecido

### Trabajo

* Sustituir el buffer fijo de objetos por lectura incremental acotada.
* Resolver `/Length` indirecto.
* Validar offsets y tamaños con aritmética comprobada.
* Eliminar todos los posibles panics inducidos por input.
* Implementar modos `strict`, `tolerant` y `forensic`.
* Conectar `max_depth`, `max_errors` y todos los límites.
* Introducir un presupuesto compartido de recursos.
* Prohibir decodificaciones ilimitadas en APIs públicas.
* Añadir cancelación y deadline.
* Devolver diagnósticos estructurados, no solamente `Null`.
* Mantener trazabilidad entre objeto, offset, revisión y nodo AST.

### Criterio de salida

Cualquier PDF malformado devuelve resultado o error controlado, nunca panic. El modo estricto no realiza recuperaciones silenciosas. Todos los límites tienen tests que demuestran su cumplimiento.

---

## `v0.3.0-alpha` — Corpus, differential testing y fuzzing

### Trabajo

Crear targets de `cargo-fuzz` para:

* lexer;
* valores y diccionarios;
* objetos indirectos;
* streams;
* xref tables;
* xref streams;
* object streams;
* page tree;
* content streams;
* filtros;
* CMap;
* XMP/XML;
* ASN.1/CMS;
* serialización y deserialización.

Añadir:

* ClusterFuzzLite en cada PR;
* campañas programadas;
* minimización de crashes;
* corpus de regresión;
* corpus pequeño incluido en el repositorio;
* descarga reproducible de corpus externos;
* job de CI que falle cuando no esté disponible el corpus;
* comparación diferencial con al menos dos parsers consolidados;
* métricas de aceptación, divergencia, tiempo y memoria.

### Criterio de salida

* Cero panics en el corpus soportado.
* Cero findings pendientes del fuzzing.
* Cada crash convertido en test de regresión.
* Resultados publicados sobre un corpus de miles de PDFs.
* Uso de memoria y tiempo medidos por percentiles, no solo mediante benchmarks aislados.

---

## `v0.4.0-beta.1` — Conformidad y codecs

### Trabajo

* Crear matriz cláusula por cláusula de ISO 32000-1 e ISO 32000-2.
* Clasificar cada característica como completa, parcial, pass-through o no soportada.
* Terminar o retirar del soporte público JBIG2.
* Diferenciar claramente inspección JPX de decodificación JPEG 2000.
* Completar CMap, `ToUnicode`, encodings y métricas de fuentes.
* Mejorar la resolución de recursos heredados en el page tree.
* Validar correctamente revisiones incrementales e híbridas.
* Comparar PDF/A y PDF/UA contra veraPDF.
* Fusionar las dos implementaciones de compliance.
* Emitir identificadores de reglas normativas.
* Implementar pruebas positivas y negativas por regla.

### Criterio de salida

Existe una tabla pública de conformidad con porcentajes reproducibles y limitaciones conocidas. Ninguna función parcial se presenta como soporte completo.

---

## `v0.5.0-beta` — API estable y arquitectura de crates

Reorganizaría el proyecto aproximadamente así:

```text
crates/
  pdf-core/          # tipos, spans, objetos y modelo lossless
  pdf-parser/        # parser, resolver y recuperación
  pdf-filters/       # filtros y codecs
  pdf-ast/           # grafo semántico
  pdf-security/      # reglas, IOCs y sanitización
  pdf-crypto/        # cifrado, firmas, CMS y certificados
  pdf-compliance/    # PDF/A y PDF/UA
  pdf-cli/           # aplicación principal
  pdf-capi/          # ABI C
  pdf-python/        # bindings Python
  pdf-node/          # bindings Node
experimental/
  pdf-diff/
  pdf-text-intel/
```

### Trabajo

* Versionar formalmente el esquema AST.
* Añadir migraciones de esquema.
* Preservar `ObjectId`, offsets y revisiones en round-trips.
* Diseñar API de consulta y visitantes estable.
* Añadir `cargo-semver-checks`.
* Reducir las features por defecto.
* Separar red y validación remota de certificados.
* Hacer OCSP, CRL y TSA explícitamente opt-in.
* Documentar compatibilidad, MSRV y política de deprecación.

### Criterio de salida

Un consumidor puede actualizar entre versiones beta sin romper su integración ni perder información al serializar y deserializar el AST.

---

## `v0.6.0-rc` — Bindings y cadena de suministro

### Trabajo

* Definir una ABI C estable con ownership y errores claros.
* Publicar wheels Python precompiladas mediante `maturin`.
* Probar Python en Linux, macOS y Windows.
* Revisar Neon o migrar a `napi-rs`.
* Publicar paquetes Node precompilados por plataforma.
* Añadir smoke tests reales de instalación.
* Automatizar publicación en crates.io, PyPI y npm.
* Crear GitHub Releases.
* Firmar tags y artefactos.
* Generar SBOM.
* Publicar checksums y provenance attestations.
* Construcciones reproducibles.

### Criterio de salida

Un usuario instala Python o Node sin necesitar toolchain Rust y obtiene exactamente el mismo comportamiento que desde la API Rust.

---

## `v1.0.0` — Condiciones mínimas

Yo no publicaría una 1.0 hasta cumplir todo lo siguiente:

* Cero placeholders en funcionalidades anunciadas.
* Cero panics conocidos ante inputs no confiables.
* Límites de recursos aplicados a todas las operaciones.
* Modos strict, tolerant y forensic claramente diferenciados.
* Corpus público con resultados reproducibles.
* Fuzzing continuo.
* Differential testing.
* Tabla de conformidad y limitaciones.
* API y esquema estables.
* Política SemVer.
* Release firmada y reproducible.
* `SECURITY.md` y proceso de divulgación.
* Bindings probados en CI.
* Al menos uno o dos consumidores externos utilizando el parser sobre cargas reales.

---

# Orden exacto en el que trabajaría ahora

1. **Corregir licencia, nombre, enlaces y claims del README.**
2. **Crear el workspace e introducir todos los crates en CI.**
3. **Mover o desactivar `pdf-diff` y `pdf-text-intel`.**
4. **Eliminar el xref artificial y cualquier dato inventado.**
5. **Corregir bounds checks en object streams.**
6. **Sustituir la lectura fija de 64 KiB.**
7. **Implementar resolución real de `/Length`.**
8. **Hacer strict realmente estricto.**
9. **Conectar el presupuesto de recursos a parser, resolver, filtros y scanner.**
10. **Añadir `cargo-fuzz` y ClusterFuzzLite.**
11. **Hacer obligatorio un corpus en la CI de conformidad.**
12. **Publicar `v0.1.1-alpha`, dejando explícito qué es experimental.**

## Qué no priorizaría todavía

No invertiría ahora mismo más esfuerzo en:

* nuevos bindings;
* OCR;
* inteligencia artificial;
* renderer;
* más perfiles de compliance;
* nuevos formatos de reporte;
* más algoritmos criptográficos;
* interfaz web.

Primero hay que conseguir que el núcleo sea **correcto, acotado, medible y creíble**.

# Conclusión

`pdf-core` tiene una base mejor de lo que su nota global puede sugerir. El problema no es que falte código: **hay demasiado alcance abierto y poca evidencia de corrección para la superficie que anuncias**.

La decisión adecuada es convertirlo durante los próximos hitos en:

> **El parser AST de PDF orientado a seguridad y forense más fiable del ecosistema Rust.**

Tu siguiente release debería ser una **`v0.1.1-alpha` de consolidación**, no una versión con más funciones. Después, `v0.2` debe concentrarse exclusivamente en seguridad del parser, semántica estricta y límites; `v0.3` en corpus y fuzzing; y solo entonces tendría sentido llamarlo beta.

[1]: https://github.com/veraPDF/veraPDF-corpus "https://github.com/veraPDF/veraPDF-corpus"
[2]: https://github.com/verapdf/verapdf-validation-profiles "https://github.com/verapdf/verapdf-validation-profiles"
[3]: https://github.com/pdf-association/pdf-corpora "https://github.com/pdf-association/pdf-corpora"
[4]: https://google.github.io/oss-fuzz/getting-started/ "https://google.github.io/oss-fuzz/getting-started/"
[5]: https://google.github.io/clusterfuzzlite/ "https://google.github.io/clusterfuzzlite/"
[6]: https://google.github.io/oss-fuzz/getting-started/accepting-new-projects/ "https://google.github.io/oss-fuzz/getting-started/accepting-new-projects/"
