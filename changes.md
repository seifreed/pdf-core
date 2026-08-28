# Veredicto

## Estado de seguimiento (2026-08-27)

Este documento conserva la auditoria original. Desde entonces se han cerrado
varios puntos: el workspace incluye todos los crates, `main` esta protegida,
las alertas de Dependabot estan resueltas, el parser tiene lecturas acotadas y
validacion de limites en las rutas de xref/object streams, y la serializacion
restaura el estado documental lossless disponible. Tambien existe un corpus
reproducible de 2.806 fixtures upstream, tres regresiones y un fixture derivado,
metricas
diferenciales, una campana fuzz local de 16.000 ejecuciones sin crashes y un
chequeo completo de 205 mapeos negativos contra veraPDF 1.30.2 y once pares
positivos/negativos de reglas locales, diez con ID veraPDF exacto; la cobertura positiva exacta por regla
sigue sin estar disponible para los 95 IDs porque veraPDF no emite resúmenes
de reglas aprobadas.
La rama `main` queda protegida en GitHub con checks estrictos de CI, corpus,
diferencial, fuzz, API y bindings; tambien se exige resolver conversaciones y
se bloquean force-pushes y borrados.
La extraccion de texto resuelve ahora fuentes indirectas, descendientes CID,
`ToUnicode`, encodings y metricas basicas de glifo dentro de su alcance
experimental.

Actualizacion 2026-08-28: `2dc820d` anade un fixture PDF/UA derivado que
rompe deliberadamente el nombre del diccionario de metadata, con evidencia
veraPDF exacta para `ISO 14289-1:2014:7.1:8`; el corpus reproducible queda en
`2.810` PDFs y la suite de mapeo pasa `6/6` con doce pares locales, once de
ellos con ID veraPDF exacto. El mapeo de `PDF_A_OUTPUT_INTENT` queda enlazado
al caso normativo `6.2.3.3:1`. `a42b4fd` publica ese mapeo corregido en el
manifiesto del corpus. El descargador y los workflows quedan fijados a
`a42b4fddba72e35c65410cfe64a5c95bbbbb828f`.
El ABI C pasa a `2.0`: longitudes y contadores publicos usan `uint64_t` y los
booleanos `uint8_t`, con asserts de layout y smoke test compilable con warnings
estrictos. Es un bump mayor deliberado porque cambia firmas y layout respecto
al contrato C `1.0` no publicado.
La evidencia local de bindings queda ampliada: wheel Python instalado en un
entorno virtual y `python/test_bindings.py` pasan con CPython 3.14 en macOS
arm64; el binding Node pasa `npm test` y `test/package-smoke.js` tras instalar
el paquete nativo localmente con Node 26.7.0. Esto no sustituye los seis jobs
multiplataforma ni la publicación en registros.
El smoke del ABI C también pasa en macOS arm64 compilando `tests/ffi_header_smoke.c`
contra `libpdf_ast.dylib`, con salida `pdf-ast 0.2.0-alpha.1: nodes=5 root=0`.
`cargo semver-checks check-release --package pdf-ast --baseline-version 0.1.0`
también pasa localmente: no requiere cambio SemVer y deja `254` checks omitidos
por tratarse de un salto mayor alpha; esto no estabiliza todavía la API pública.
Actualización posterior: `b74af07` hace que el agotamiento de `max_depth` en el
árbol de páginas se propague como `ResourceBudgetError::Depth` en la API
presupuestada, en lugar de devolver una lista truncada sin error. La regresión
focalizada, el formato y Clippy estricto pasan. El último push (`b74af07`)
disparó de nuevo los siete workflows; al redactar esta entrada aún estaban
pendientes o en cola, por lo que no se cuentan como evidencia remota verde.
Los commits `37bbb6e`, `e4a9f3f`, `c9dbd0c`, `d58b174`, `79b4b98` y `e4903db`
extienden `ResourceBudget` a la recursión de valores, objetos indirectos,
prefijos de streams y lotes tolerantes, evitando que un `TooLarge` estructural
se convierta en recuperación silenciosa. También se comprueba de forma
temprana que el presupuesto pueda retener el documento lossless. El workspace
queda en `514` tests verdes en `69` suites; esto endurece la frontera de parseo,
pero no completa todavía todas las rutas de recuperación ni la conformidad
normativa.
El repositorio `pdf-core-corpus` añade además ocho pares PDF/A y un par PDF/UA
verificables en `51c53d5ac34cdb0a3ad8179ccf02b3e98608d5bc`, elevando
`RULE-COVERAGE.json` a `21` mappings con positivo/negativo local, resultado
documental de veraPDF e ID exacto para el fallo negativo. `0839ca2` fija ese
commit en el descargador y en los workflows de corpus.
La verificación local completa del corpus actualizado pasa `2.810/2.810`
sin panics ni errores de parseo, con `2.810` hashes, `161281039` bytes,
`peak_rss_kib=474480` y percentiles `3/57/178 ms`. La comparación veraPDF
pasa `569/569`, con `263` conformes, `565` rechazos strict y `0` divergencias
tolerantes. El diferencial pasa `2.810/2.810` y registra `397` divergencias
(`396` desacuerdos entre referencias y `1` de consenso), `peak_rss_kib=312112`
y percentiles `31/62/102 ms`. La matriz reproducible las clasifica en `389`
casos `core_mutool_only`, `7` `core_qpdf_only` y `1` `core_only`; los otros
cinco estados quedan en cero. Falta todavía explicar la causa semántica de
cada grupo, pero ya no se confunden desacuerdos de referencia con divergencias
de consenso.
`b56e861` corrige la clasificación de streams: el AST de contenido solo
intenta parsear streams referenciados desde `/Contents` de páginas, evitando
interpretar perfiles ICC, fuentes u otros datos binarios como operadores en
modo strict; la regresión del resolver pasa `22/22` y el smoke strict real
queda verde.

Actualizacion posterior: `b410aa5` deduplica referencias repetidas durante la
resolucion y reutiliza nodos semanticos ya asociados a `ObjectId`; `62adb2a`
completa el cobro de memoria para objetos indirectos publicos. `da39970` y
`15cfaa3` corrigen los offsets de los PDFs minimos usados por los smoke tests
Python y de integracion, y `aaaa828` evita enlazar los tests del crate Python
en el build nativo release de macOS. El workspace pasa 428 tests, el corpus
completo local pasa 2.809 fixtures con 2.126 errores controlados, y el
diferencial completo registra 397 divergencias en 142.311 ms. Los nuevos
workflows remotos para `aaaa828` siguen pendientes; no se cuentan como verdes
hasta finalizar.

Actualizacion 2026-08-27: la campana de ClusterFuzzLite encontro y ya tiene
corregidos dos crashes reproducibles en entradas malformadas, uno en hex
invalido de CMap y otro en cabeceras JBIG2 truncadas. Las correcciones estan
en `9b640bb` y `44dd805`, con regresiones locales y fuzz dirigido en verde.
El validador PDF/A ahora sigue recursos heredados desde `Pages`, incluyendo
referencias indirectas y ciclos controlados, con regresion dedicada.
La matriz de CI tambien instala qpdf/MuPDF para el diferencial, conserva el
log y las metricas de memoria como artefacto, y ejecuta el corpus externo
completo. Los workflows del ultimo push siguen en cola; hasta que terminen no
se debe contar esta evidencia como verde.

Avances publicados posteriormente: `397b8ae` conserva la identidad `ObjectId`
de los nodos semanticos resueltos y valida que las fuentes PDF/A apunten a
streams reales; `93dfce8` rechaza bytes residuales en objetos de object streams
en modo `Strict`; `d3cbb31` rechaza bytes XMP que no sean UTF-8; y `a3401e0`
mas `e3f0d83` protegen la aritmetica de rangos y la entrada UTF-8 de CMap.
`bb624f1` completa la preservacion del mapeo `ObjectId` en el round-trip de
serializacion tambien para nodos semanticos, no solo para `NodeType::Object`.
El push de `b52da72` tuvo el corpus externo en verde (run `33059191326`),
mientras que el diferencial, ClusterFuzzLite y API Stability seguian en
ejecucion y CI, bindings y fuzzing seguian en cola.
Hasta que termine todo el conjunto no se debe contar la evidencia remota como
verde.

El chequeo local de `cargo-semver-checks` sigue pasando contra `pdf-ast`
`0.1.0` (254 checks omitidos por tratarse de un salto mayor alpha).

Actualizacion de evidencia para `6a63d85`: CI completo verde (`33075200727`),
corpus externo con veraPDF (`33075200739`), diferencial (`33075201013`),
fuzzing de los 16 targets (`33075200762`), ClusterFuzzLite (`33075200736`)
y API Stability (`33075200729`). Los smoke tests de bindings de Linux y macOS
tambien estan verdes dentro de `33075200753`; los dos jobs de Windows seguian
en curso al redactar esta entrada. El test local aislado de validacion pasa
20/20; el timeout observado en la ejecucion global fue de compilacion
acumulada antes de esa suite, no un fallo funcional. Los commits
`d915906`, `ea0dd28`, `b69ea73`, `7dfbcb9` y `6a63d85` endurecen la validacion
strict del arbol de paginas, recursos y catalogo; `fc25e84` corrige la
semantica de streams ya decodificados sin reaplicar filtros.

Avances locales posteriores: `1eeec86` rechaza o diagnostica nodos `/Pages`
sin `/Kids` o `/Count`; `7790b10` aplica la misma regla a `/Parent` de las
hojas `/Page`; `b6a14f0` expone consultas de paginas con `ResourceBudget`; y
`50bff65` resuelve referencias indirectas del catalogo durante las reglas
PDF/A de JavaScript, XFA y metadata. Todos tienen regresiones y pasan Clippy.
`e0339df` conserva tambien esos `ObjectId` al crear los nodos indirectos reales
de `OpenAction`, `Names`, `AcroForm` y `Metadata` durante el parseo.
`1e378c9` rechaza ademas que el objeto apuntado por `/Catalog /Pages` sea una
hoja `/Page` en vez del nodo raiz `/Pages`; `84ebab3` mantiene el fixture de
ciclos con la estructura minima necesaria. Las nuevas regresiones dejan
`27/27` integraciones y `21/21` validaciones locales en verde; el smoke aislado
de veraPDF pasa `1/1` con el corpus local.
`d5f5ce3` acepta tambien `/Kids` como referencia indirecta a un array, y
`d62d062` procesa diccionarios `/Names` inline ademas de los indirectos; ambos
casos tienen regresiones serializadas y mantienen Clippy estricto en verde.
`a15f5f0` valida que `/Count` sea un entero no negativo, incluso cuando el
valor es indirecto, con regresion strict/tolerante.
`4e12ad5` conecta `OutputIntentsParser` al flujo principal despues de resolver
referencias, y `deb999a` valida en PDF/A-1b que el array no este vacio y que
cada salida declare `GTS_PDFA1`, identificador y perfil ICC resoluble. La
regresion combinada de integracion y validacion pasa `51/51` y Clippy estricto.
`ab1de0d` hace que la regla PDF/UA de idioma resuelva tambien un `/Lang`
indirecto, evitando marcar como ausente un idioma valido; su regresion pasa
`23/23` en la suite de validacion y Clippy estricto.
`4178a95` hace que la regla PDF/UA de texto alternativo exija una cadena no
vacia ni compuesta solo por espacios, y resuelva valores indirectos; su
regresion pasa `24/24` en validacion, `53/53` en integracion y validacion
combinadas, y Clippy estricto.
`06bda33` hace que la regla de estructura etiquetada PDF/UA resuelva tambien
`MarkInfo` y `StructTreeRoot` indirectos; su regresion pasa `25/25` en
validacion, `54/54` en las suites combinadas y Clippy estricto.
`915fa1c` deja de convertir en cero los contadores invalidos de bloques CMap y
propaga el error de parseo; su regresion pasa `11/11` en las pruebas de CMap y
Clippy estricto.
`1e2fedb` hace que la extraccion de texto resuelva tambien el array indirecto
`DescendantFonts`, manteniendo la resolucion de sus fuentes CID; la regresion
pasa `5/5` en text extraction y Clippy estricto.
`304f17d` comprueba el tamano real devuelto por la decodificacion JPX antes de
retornar la imagen, evitando superar el limite aunque difiera del calculo de
cabecera; la suite pasa `4/4` y Clippy estricto.
`3c4892d` rechaza tambien valores `/Lang` vacios o compuestos solo por
espacios; la suite PDF/UA pasa `25/25` y Clippy estricto.
`d122cb4` propaga los errores de entradas CMap malformadas y rechaza bloques
truncados cuyo contador declara mas lineas; la suite focalizada pasa `12/12`
y Clippy estricto.
`1005b2e` hace que `StructTreeParser` materialice `Lang`, `Alt` y
`ActualText` cuando sus valores son referencias indirectas; la regresion pasa
`4/4` en la suite de estructura y Clippy estricto.
`53f80fb` resuelve arrays indirectos `Nums`, `Kids` y `Limits` del `ParentTree`
de estructura etiquetada; la regresion pasa `5/5` en la suite de estructura y
Clippy estricto.
`e426a22` hace que la extraccion de texto resuelva tambien el array indirecto
`W` de anchos CID; la regresion de fuentes pasa `5/5` y Clippy estricto.
`b6406da` aplica la misma resolucion a `/Widths` de fuentes Type1; la
regresion de anchura PDF pasa `5/5` y Clippy estricto.
`6b54f55` resuelve tambien `/FirstChar` indirecto al calcular anchuras Type1;
la regresion de anchura PDF pasa `5/5` y Clippy estricto.
`de838af` hace que `StructTreeParser` resuelva `RoleMap` indirecto; la
regresion de estructura pasa `5/5` y Clippy estricto.
`f14c494` aplica la misma resolucion a `ClassMap` indirecto y conserva sus
entradas de clase; la regresion conjunta de estructura pasa `5/5` y Clippy
estricto.
`ba531ed` resuelve tambien nombres indirectos como valores de `RoleMap`; la
regresion conjunta de estructura pasa `5/5` y Clippy estricto.
`eef5f0d` recorre `K` cuando apunta indirectamente a arrays de hijos, incluidos
arrays anidados; la suite de estructura pasa `6/6` y Clippy estricto.
`22417e5` conserva tambien entradas `ClassMap` inline con arrays de clases,
sin perderlas al materializar la estructura; la suite pasa `6/6` y Clippy
estricto.
`839981d` valida que `/ToUnicode` apunte realmente a un stream, tambien cuando
la referencia indirecta resuelve a otro tipo; la suite de fuentes pasa `2/2`
y Clippy estricto.
`da55ad4` valida que `/Encoding` sea un nombre o diccionario real, incluidos
valores indirectos, y diagnostica tipos invalidos; las suites de fuentes y
PDF/UA pasan `28/28` y Clippy estricto.
`d0f0dbd` resuelve `/DW` indirecto para el ancho por defecto de fuentes CID;
la regresion de extraccion pasa `5/5` y Clippy estricto.
`5b02fed` resuelve arrays `Differences` indirectos en encodings de fuentes;
la regresion de extraccion pasa `5/5` y Clippy estricto.
`ccd773b` conecta `IDTree` con el parser de name trees existente y conserva
sus nombres y referencias; las suites de estructura y name trees pasan `6/6`
y `4/4`, respectivamente, con Clippy estricto.
`1b56b79` resuelve arrays indirectos `Names`, `Kids` y `Limits` en name trees,
incluido `IDTree`; las suites de name trees y estructura pasan `5/5` y `6/6`,
respectivamente, con Clippy estricto.
`2bfd6f4` resuelve entradas `Nums` indirectas que contienen arrays de padres y
recorre niveles anidados de `Kids` en `ParentTree`; la suite de estructura pasa
`8/8` y Clippy estricto.
`d7c6135` resuelve referencias indirectas en campos de estructura y MCR
(`S`, `Type`, `MCID`, `Pg` y `Obj`); la suite de estructura pasa `9/9` y
Clippy estricto.
`2c61d80` resuelve metadata de fuentes indirectas (`Subtype`, `BaseFont`,
`Encoding`, `FontMatrix`, anchos y diferencias); la suite de extraccion pasa
`5/5` y Clippy estricto.
`5125ff0` resuelve valores indirectos de `Kids`, `Resources`, `Contents` y
`Annots` en el arbol de paginas, incluidos tipos de fuentes y XObjects; la
regresion adicional `ec971d6` deja la suite de page tree en `4/4`, con Clippy
estricto.
`bf7290e` clasifica tambien definiciones `ColorSpace` indirectas (`Indexed`,
`ICCBased`, `Separation`, `DeviceN` y `Pattern`); la suite de page tree pasa
`4/4` y Clippy estricto.
`3f81070` resuelve tambien claves y límites de `NameTree` cuando son cadenas
indirectas; la suite de name trees pasa `5/5` y Clippy estricto.
`71682f3` resuelve valores numericos indirectos en `ExtGState` (`LW`, `LC`,
`LJ` y `OPM`); la suite de ExtGState pasa `2/2` y Clippy estricto.
`add2fa2` rechaza claves negativas en `ParentTree/Nums` en lugar de
convertirlas silenciosamente a `u32`; la suite de estructura pasa `9/9` y
Clippy estricto.
`2e9a5c2` evita que `parse_object` convierta una cabecera indirecta truncada
en un valor parcial, tambien en modo tolerante; `parser_tests` pasa `32/32` y
Clippy estricto.
`3cfedf6` conserva la `/Length` indirecta resuelta en
`PdfStream.lossless.declared_length`, sin mutar el diccionario ni perder
`observed_length`; la suite de reference resolver pasa `14/14` y Clippy
estricto.
La comprobacion local posterior pasa `279` tests en `7` suites de libreria con
`cargo test --workspace --locked --lib -- --test-threads=1`; el CI remoto sigue
encolado y esta cifra no sustituye la evidencia de corpus completo.
`5b23a0b` hace que la ruta de produccion resuelva arrays `Annots` indirectos
antes de procesar sus anotaciones; la suite de `pdf_file` pasa `21/21` y Clippy
estricto.
`31bfd07` hace que la ruta de produccion resuelva y fusione categorias de
recursos indirectas durante la herencia, incluida `Font`; la suite de
`pdf_file` pasa `22/22` y Clippy estricto.
`b7a4ac2` conserva tambien la categoria heredada cuando la definicion hija es
un diccionario vacio; la misma suite pasa `22/22` y Clippy estricto.
`c47985d` añade variantes acotadas de CMap que distinguen sintaxis invalida de
agotamiento de `ResourceBudget`, y propaga este ultimo en resolver y
extraccion; las suites pasan `13/13`, `14/14` y `5/5`, con Clippy estricto.
`bc3c183` alinea el commit por defecto del descargador de corpus con
`532c2b6`, la revision reproducida por CI para los `2.809` fixtures.
Verificacion local contra esa revision: el corpus externo pasa `2.809/2.809`
sin panics, con `2.809` hashes comprobados, `1.990` errores controlados y
`peak_rss_kib=286160` (`p50=0 ms`, `p95=7 ms`, `p99=52 ms`). La comparacion
veraPDF 1.30.2 pasa `569/569`, con `263` conformes, `565` rechazos strict y
`0` divergencias tolerantes. El diferencial completo contra qpdf y MuPDF
registra `397` divergencias (`396` desacuerdos entre referencias y `1`
divergencia de consenso), con `peak_rss_kib=428240` y percentiles
`27/83/196 ms` (`p50/p95/p99`); esas divergencias siguen siendo pendientes de
clasificar, no findings de crash.
La suite de mapeo reproducible contra veraPDF pasa `6/6`: cubre los `205`
casos negativos Isartor, los casos PDF/UA publicados y los `9` pares
positivos/negativos de reglas locales.
`c3ff7c1` propaga tambien los excesos de nodos de `ResourceBudget` desde
`OutputIntentsParser` hasta la ruta principal; su regresion pasa `1/1`, la
suite de `pdf_file` `22/22` y Clippy estricto.
Las regresiones siguientes completan la misma garantia en ICC/output intents,
recuperacion de revisiones, object streams, carga de objetos, content streams,
JBIG2Globals, xref, objetos malformados, linealizacion y resolucion transitiva:
`a75bcd2`, `551c6a7`, `0302f79`, `6a0f346`, `44f6904`, `13f8e6a`, `9203503`,
`8517d21`, `636c571` y `a1f3b39`. Las suites focalizadas pasan `26/26` en `pdf_file`,
`20/20` en `reference_resolver` y `284/284` tests de libreria en `pdf-ast`.
`7edcfec` corrige el doble cobro del presupuesto de objetos al inspeccionar el
prefijo de un stream antes de su parseo completo; la regresion de streams,
el resolver completo (`21/21`), Clippy estricto y la suite de libreria
(`301/301` en `7` suites) pasan.
`46b1813` evita que el modo tolerante oculte agotamientos de `ResourceBudget`
durante la resolucion final de referencias; su regresion de `pdf_file` pasa
`1/1`, la suite focalizada `27/27` y Clippy estricto.
La suite completa local posterior pasa `282` tests en `7` suites de libreria
con `cargo test --workspace --locked --lib -- --test-threads=1`.
La verificacion posterior a `d24cf35` pasa `303` tests en `7` suites del
workspace con el mismo comando.
`750d48c` expone `ColorSpaceParser::parse_colorspace_with_budget` y propaga
los excesos de nodos/aristas de espacios de color desde la resolucion de
recursos de pagina; la regresion de colores pasa `2/2`, el smoke ICC `1/1`,
la suite de libreria del workspace y Clippy estricto pasan.
`68690b4` hace que el nodo de metadatos de perfiles ICC informe tambien el
agotamiento de nodos al mismo presupuesto compartido.
`c5e7092` propaga tambien los errores de decodificacion presupuestada de
perfiles ICC, distinguiendolos de un perfil invalido; la suite de colores pasa
`3/3` y Clippy estricto.
`4330704` añade una regresion de la ruta `ReferenceResolver -> Page resources`
que verifica la propagacion de `Nodes` para espacios de color; el caso pasa
con Clippy estricto.
`7f44c11` añade variantes presupuestadas a la construccion inicial de
`NameTreeParser` y deja de ocultar agotamientos de nodos/objetos; la suite de
name trees pasa `6/6` y Clippy estricto.
`e0ee4e4` añade `StructTreeParser::parse_struct_tree_root_with_budget` y
propaga durante la construccion los limites de nodos, objetos, aristas y
contenido MCID, conservando el wrapper legacy parcial; la suite de estructura
pasa `10/10` y Clippy estricto.
La verificacion posterior de `cargo test --workspace --locked --lib
-- --test-threads=1` pasa `273` tests del crate principal y `14` de crates
auxiliares; los workflows remotos del push siguen pendientes.
`948943c` expone `FunctionParser::parse_function_with_budget` y deja de
ocultar el agotamiento de nodos al materializar funciones anidadas; la suite
de funciones pasa `3/3` y Clippy estricto.
`19622f1` expone variantes presupuestadas para construir `OCProperties` y
`OCMD`, preservando los wrappers legacy; la suite OCG pasa `2/2` y Clippy
estricto.
`819a14e` expone `PageTreeParser::parse_page_tree_with_budget` y propaga los
limites de nodos, aristas, cancelacion/deadline y decodificacion ICC durante
la herencia y procesamiento de recursos; la suite de page tree pasa `5/5` y
Clippy estricto.
`e4f0973` expone `ExtGStateParser::parse_extgstate_with_budget` y propaga los
limites de nodos y aristas de mascaras/halftones inline; la suite de ExtGState
pasa `3/3` y Clippy estricto.
La verificacion completa posterior pasa `274` tests del crate principal y
`14` de crates auxiliares con `cargo test --workspace --locked --lib
-- --test-threads=1`.
Tras `PageTreeParser` y `ExtGStateParser`, la verificacion completa vuelve a
pasar `276` tests del crate principal y `14` de crates auxiliares.

Avances adicionales publicados: `39de16a` limpia los handles de salida del
ABI C cuando una operación falla; `2e05453` cobra la entrada de todos los
streams filtrados; `e79ee6e` y `ae2eafc` acotan cifrado/descifrado, incluido el
peor caso de expansión AES; `6a18db2` cobra la materialización de XML XMP;
`77ddaf4` añade importación del grafo AST con presupuesto; y `d5f1df7` más
`99a0571` hacen que las migraciones de esquema alcancen el target exacto y
terminen ciclos sin progreso. `d8d9d36` corrige el sentinel de la regresion
FFI para mantener Clippy estricto en verde. `b6da322` fija a `int32_t` el
ancho de los códigos de error del ABI C y lo comprueba en el smoke test.
`05c496e` unifica las 13
comprobaciones PDF/A-1b en la misma colección de constraints que consume el
registro, sin alterar sus códigos ni severidades. `e34d732` propaga los
recursos efectivos por niveles `Pages` anidados y añade una regresión que
verifica la herencia y la creación de recursos derivados. Sus workflows aun
estan pendientes o en cola.

`8f496b6` añade al workflow de fuzzing un artefacto de recursos por target y
registra el `Maximum resident set size` en cada resumen. `f96e169` conserva
además el resumen y marca `findings=1` cuando libFuzzer termina con error,
antes de devolver el fallo del job; la métrica queda disponible para la
próxima ejecución verde de los 16 targets. `2775aeb` conserva la misma
métrica de memoria para el smoke completo del corpus veraPDF.

Avance publicado en `f2e8a5f`: el smoke test de instalación JavaScript ejecuta
la carga del addon nativo en un proceso hijo, evitando el bloqueo `EPERM` de
Windows durante la limpieza del directorio temporal. Avance publicado en
`76d9fd6`: cada `PdfStream` conserva ahora, cuando están disponibles, sus
bytes originales, longitud declarada y observada, errores de parseo, acciones
de recuperación y estado raw/decoded/lazy; el estado se conserva al
serializar y al convertir streams durante desencriptación o resolución de
JBIG2Globals. Ambos cambios tienen regresiones locales.

Avance adicional: `SerializableGraph` y `SerializableDocument` exponen ahora
variantes `*_with_budget` que cargan nodos, aristas, revisiones y payloads de
streams contra `ResourceBudget`; la deserializacion de AST y documento tambien
expone variantes acotadas. Visitantes, consultas y walkers de nodos, aristas y
revisiones tienen la misma variante acotada, incluyendo las entradas xref de
documentos serializados. CMap aplica
`UseCMap` cuando el base ya fue parseado, acepta hex PDF de longitud impar y
limita el número de mappings procesados. El smoke C se compila en CI con
`-Wall -Wextra -Werror`.

Avance local adicional: el parser `Strict` rechaza tokens no consumidos en
`parse_value` y `parse_object`; `PdfAstGraph` y `NameTreeParser` terminan sus
recorridos aunque el grafo tenga ciclos; y los helpers publicos legacy de
`PageTreeParser` y `OutputIntentsParser` usan el mismo `ResourceBudget` para
decodificar perfiles ICC. `StructTreeParser` y `OutlineParser` tambien limitan
profundidad y ciclos durante la construccion y la consulta de sus arboles, y
presupuestan sus aristas. `NameTreeParser`, `ExtGStateParser` y
`OutputIntentsParser` tambien acotan las creaciones de nodos y aristas bajo el
mismo presupuesto, y `FunctionParser` acota tambien la creacion de funciones
anidadas. `ContentStreamParser` y sus funciones de operandos acotan bytes de
entrada y numero de operadores bajo `ResourceBudget`. La resolucion de
`/Length` indirecto conserva ahora
el diccionario declarado y todos los bytes observados, registrando la longitud
resuelta como metadato separado. `PageTreeParser` acota tambien los nodos de
color space y perfiles ICC inline, ademas de sus aristas de recursos. Cada
punto tiene regresion y pasa Clippy estricto. Las APIs publicas de XMP aceptan
ahora un `ResourceBudget`, y el parser PDF lo propaga al parseo del paquete.
Las APIs publicas de `PdfParser` para valores, objetos y secuencias tambien
consumen la entrada del presupuesto antes de analizarla, con regresion.
Esas mismas operaciones consumen ahora el contador `max_objects` una vez por
valor de nivel superior, incluidos valores directos y secuencias recuperables.
`ContentAnalyzer::analyze_content_stream_with_budget` cobra la entrada antes
de analizarla y cada nodo antes de materializarlo, evitando copias iniciales
del stream; la API legacy conserva el presupuesto por defecto.
`CMapParser::decode_bytes_with_budget` cobra la secuencia de códigos y el
texto Unicode resultante, y `decode_bytes` conserva la API legacy con límites
por defecto.
`TextExtractor::extract_text_with_budget` comparte ese presupuesto con CMap y
`ToUnicode`, cobra operadores y spans, y la API legacy conserva límites por
defecto.
`LazyStream::load_with_budget` y `to_stream_with_budget` cobran lecturas,
clones, buffers padres de object streams y datos cacheados antes de
materializarlos.
Las consultas publicas de `NameTreeParser` exponen variantes con presupuesto
para recopilar nombres, buscar entradas, extraer JavaScript y describir
archivos embebidos; las APIs legacy conservan resultados parciales al agotar
el presupuesto.
La materializacion y el aplanado de outlines exponen tambien variantes con
presupuesto que limitan objetos y bytes de salida, manteniendo las APIs legacy
con resultados parciales.
Las consultas de texto de `StructTreeParser` y el almacenamiento de contenido
MCID cobran objetos y bytes antes de materializar resultados o metadatos.
Las colecciones y recorridos públicos de `PdfAstGraph` ofrecen variantes con
presupuesto para nodos, aristas, rutas, BFS/DFS y walkers; las APIs legacy
mantienen wrappers acotados con los límites por defecto.
Las consultas de profundidad y numeración de páginas del grafo comparten ahora
el mismo límite de nodos y terminan con error explícito en sus variantes
presupuestadas.
El parseo de cabeceras de object streams limita tambien el numero de entradas
con el mismo presupuesto en sus rutas publicas e internas.
El parser publico de imagenes inline tambien rechaza entradas que exceden el
presupuesto antes de clonar los datos.
El resolver ya no parsea content streams con un presupuesto por defecto ni
silencia sus fallos: comparte el presupuesto, conserva el error en el nodo y
en el estado lossless del stream, y propaga el fallo en modo `Strict`.
Los parsers publicos de valores, objetos indirectos y prefijos de streams en
`parser::object_parser` aplican ahora el presupuesto estandar y exponen
variantes con `ResourceBudget`, con regresion dedicada. Las tablas xref
publicas y la deteccion de xref hibrido tambien cobran cada entrada contra el
mismo presupuesto.
La seleccion de esquemas en `validate_all` ya no aplica PDF/A-1 a documentos
1.7/2.0 ni el esquema PDF 2.0 a documentos 1.x.
Los perfiles PDF/X tambien exigen ahora la version PDF base que corresponde a
cada familia, evitando aceptar versiones posteriores incompatibles.
PDF/UA-1 y PDF/UA-2 siguen la misma regla de version base y ya no se aplican
entre si por una simple comparacion de version minima.
`SchemaRegistry::validate` comparte ahora ese filtro y devuelve `None` para
perfiles desconocidos o incompatibles con la version del documento.
El adaptador `pdf-compliance` expone tambien PDF/UA-2 con su funcion de
validacion y smoke test para documentos PDF 2.0.
Los trailers asociados a tablas xref tambien se decodifican con el presupuesto
compartido, y la ruta documental rechaza entradas residuales antes de
`trailer`, con regresiones dedicadas.
`PerformanceGuard` compara ahora los limites de fichero, objeto y memoria en
bytes exactos, rechaza `1 MiB + 1 byte` con un limite de `1 MiB` y no registra
asignaciones de memoria rechazadas.
Las primitives publicas de `parser::lexer` ofrecen ahora variantes
`*_with_budget` que cobran el slice de entrada antes de ejecutar nom, para
evitar asignaciones de tokens fuera de `ResourceBudget`.
La recuperación de xref cercana queda limitada a modos tolerantes; `Strict`
rechaza una sección malformada aunque exista otra tabla recuperable próxima.
Los ciclos de `/Prev` reciben ahora el mismo tratamiento: son recuperables en
modos tolerantes y errores en `Strict`, con regresión dedicada.
Los ciclos autorreferentes del árbol de páginas también se rechazan en
`Strict` y se cortan de forma controlada, con `ParseDiagnostic`, en modos
tolerantes.
El exceso de profundidad en árboles de campos de formulario sigue la misma
política: error en `Strict` y diagnóstico de rama omitida en tolerante.
La decodificación pública de `PdfStream` ya no trata filtros desconocidos o
malformados como datos sin filtrar, y los errores de `JBIG2Globals` se
propagan en `Strict` en lugar de descartarse silenciosamente; el resolver usa
la misma validación al cargar object streams, content streams, xref streams y
perfiles ICC. `DecodeParms` con tipos inválidos también se rechaza en lugar de
convertirse silenciosamente en parámetros por defecto.
`DCTDecode` ya no devuelve datos JPEG inválidos como si fueran bytes raw; los
errores de formato se propagan al límite de decodificación.
Las APIs directas de JPX y JBIG2 exponen ahora variantes `*_with_budget` que
cobran sus buffers de entrada, `JBIG2Globals` y la salida decodificada antes de
devolverla; las APIs legacy conservan límites por defecto.
`CcittDecoder::decode_with_budget` aplica el presupuesto compartido junto con
el límite configurado del decoder y cobra la entrada y la imagen resultante.
`PredictorDecoder::decode_with_budget` cobra la entrada y la salida TIFF/PNG y
rechaza antes de decodificar cuando la salida máxima posible no cabe.
`SerializableGraph` y `SerializableDocument` exponen ahora variantes
presupuestadas para JSON y CBOR, y sus entradas JSON/CBOR comprueban el tamaño
antes de parsear y la estructura materializada después.
El parser documental de xref streams rechaza ahora en `Strict` tipos de
entrada desconocidos en lugar de convertirlos silenciosamente en entradas
libres; `Tolerant` conserva la recuperación anterior.
La API pública de xref streams consume también el contador compartido de
objetos antes de materializar cada entrada, igual que las tablas xref.
Los fallos de parseo XFA en modo tolerante quedan registrados como
diagnósticos `xfa_parse` con la acción `skipped_xfa`, además del log.
Los fallos de decodificación o XML de XMP ya no se silencian en `Strict`;
`Tolerant` los registra como `xmp_decode`/`xmp_parse` y conserva el stream.
La API `XmpMetadata::parse_from_stream_with_budget` cobra el buffer de bytes
antes de convertirlo a texto, evitando una materialización fuera del límite.
XFA ya no interpreta como XML raw un stream cuya decodificación de filtros
falló; el error se propaga al modo estricto y se registra mediante la ruta
tolerante de diagnóstico.
La resolución de arrays XFA comparte ahora comprobaciones de cancelación y
profundidad con el parser; `Strict` rechaza anidamiento excesivo y `Tolerant`
lo corta con diagnóstico.
El parser incremental de `streaming` acepta ahora un `ResourceBudget` compartido:
cobra cabecera, chunks y nodos, reutiliza el presupuesto al parsear objetos y
propaga los excesos en lugar de tratarlos como chunks malformados recuperables.
La API `PdfParser::with_resource_budget` permite reutilizar ese presupuesto en
otras entradas de parseo.
Al usar un presupuesto explícito, `PdfParser` sincroniza también los límites
derivados de fichero, memoria, stream, nodos, aristas y profundidad, evitando
que `PerformanceLimits` conserve máximos contradictorios.
`RecoveryParser` acepta el mismo presupuesto y trata los excesos de tamaño,
memoria o contadores como errores terminales; ya no intenta recuperar ni
construir un documento best-effort después de cruzar un límite de seguridad.
La fase de recovery conserva ahora el buffer original prestado hasta que una
estrategia produce datos modificados, eliminando la copia inicial completa.
La construcción best-effort de recovery reutiliza el presupuesto del parser y
limita nodos, aristas y copias de objetos antes de materializarlos.
`PdfParser::parse_bytes` cobra también la retención de `original_bytes` antes
de crear la copia lossless.
Las lecturas auxiliares de xref, objetos y escaneos del `ReferenceResolver`
reservan sus buffers mediante el presupuesto compartido antes de materializar
los bytes.
La recuperación de xref del `PdfFileParser` lee en chunks y cobra cada bloque;
la resolución de longitudes usa un buffer fijo en la pila.
El timeout configurado en `RecoveryConfig` se comprueba antes de cada
estrategia y antes del parseo final, evitando recuperaciones sin límite
temporal.
Las reservas del parser incremental se limitan al presupuesto de entrada
restante antes de asignar buffers; una configuración `chunk_size = 0` se
normaliza para mantener progreso y el agotamiento del presupuesto se propaga.
La ruta strict de content streams rechaza bytes residuales y operandos sin
operador, mientras que `Tolerant` conserva el escaneo recuperable anterior.
La misma ruta reconoce imágenes inline `BI`/`EI` y cobra sus datos antes de
crear los operadores de imagen, evitando falsos rechazos por bytes binarios.
El smoke local strict/tolerante contra 100 fixtures externos y veraPDF no
detectó divergencias del parser; el corpus completo de CI sigue pendiente.
El workflow de release verifica el tag firmado también antes de construir
bindings y attesta los binarios, paquetes, SBOM y checksums generados.
`README.md` ya no afirma que esos artefactos se publiquen: el workflow los
construye y prepara, pero la publicación en registros sigue deshabilitada.

Pendientes actuales, en orden practico:

* **Evidencia remota:** ya existe una ejecucion verde completa del CI,
  corpus/differential, fuzzing, ClusterFuzzLite y API Stability en `6a63d85`,
  y cuatro de seis smoke tests de bindings. Faltan los dos jobs Windows de
  `33075200753` y ejecuciones verdes repetidas para considerar la evidencia
  sostenida. Los workflows ya no cancelan runs anteriores del mismo ref,
  porque esa politica impedia obtener evidencia completa tras cada push; los
  IDs y estados se consultan con `gh run list` para evitar referencias
  obsoletas.
  En el PR `#23`, `33165897719` deja verde el corpus externo completo:
  `2.809` archivos, `2.809` hashes, `1.859` errores controlados,
  `peak_rss_kib=228824` y percentiles `0/12/80 ms` (`p50/p95/p99`).
  `33165897723` deja verde el diferencial completo: `2.809` archivos,
  `386` divergencias (`385` desacuerdos entre referencias y `1` de consenso),
  `peak_rss_kib=253592` y percentiles `11/81/241 ms`; esa medicion historica
  precede al corpus actual y a la matriz de clasificacion.
* **Parser endurecido:** strict y varios recorridos ya tienen guardas, incluido
  el árbol de páginas y sus referencias obligatorias, y los streams conservan
  ahora estado lossless explícito. Las APIs públicas de
  valores/objetos cobran también la memoria retenida, y la resolución usa el
  presupuesto configurado para su recorrido inicial, pero falta hacer
  completamente estrictos los modos de error/recuperacion y demostrar que
  `ResourceBudget` cubre la superficie publica restante de parseo, resolucion,
  recorrido, decodificacion y serializacion. Las variantes acotadas de
  `parser::lexer` cobran el slice completo antes de analizarlo, pero no
  sustituyen aun la frontera de parseo estructural para entradas no
  confiables. `4cfc786` evita además que los errores de presupuesto al resolver
  objetos indirectos se conviertan silenciosamente en referencias ausentes;
  `636c571` aplica la misma garantía a la detección de linealización;
  `8517d21` la conserva también durante la recuperación de objetos malformados;
  `9203503` la propaga además al escaneo de recuperación de xref; queda
  extenderla a las rutas de parseo y resolución que aún usan recuperaciones
  parciales. `0302f79` completa esta garantía en trailer y xref streams,
  incluidas sus rutas tolerantes de decode y recuperación cercana. `a75bcd2`
  propaga también los errores de decode ICC, nodos y aristas en
  `OutputIntentsParser`. `551c6a7` evita además que las rutas tolerantes de
  content streams y `JBIG2Globals` silencien agotamientos de presupuesto.
  `13f8e6a` aplica el mismo contrato al cargador principal de objetos, sus
  `Length` indirectos y object streams. `44f6904` extiende la garantía al
  cálculo de offsets dentro de object streams.
  `6a0f346` evita además ocultar esos límites durante la recuperación de
  revisiones xref, XFA y XMP. `a1f3b39` hace que la resolución transitiva de
  referencias tampoco salte errores de presupuesto en modo tolerante.
  `46b1813` aplica la misma garantía a la resolución final de referencias del
  documento, que antes convertía esos excesos en simples avisos tolerantes.
  `d24cf35` extiende el cobro de `max_objects` a las entradas de xref streams
  materializadas por `PdfFileParser`; la regresión específica y la suite
  `pdf_file` pasan `28/28`, con Clippy estricto.
  `ba189dd` rechaza bytes residuales después de la última caja JP2; la suite
  de entradas inválidas JPX pasa `4/4` y Clippy estricto. Esto endurece el
  límite estructural sin convertir JPX en soporte completo del estándar.
  `6653e25` conserva el valor real de booleanos al materializar parámetros de
  operandos de imágenes inline; la regresión focalizada pasa `3/3` y Clippy
  estricto.
  `068cd0c` implementa las posiciones solicitadas por las operaciones de
  insertar/mover hijos y mantiene sincronizadas las listas del AST al quitar
  aristas o nodos. Las regresiones de transform y graph pasan `5/5`, Clippy
  estricto queda limpio y el workspace completo pasa `306` tests.
  `2043bce` decodifica los streams XMP antes de inspeccionarlos en PDF/UA y
  convierte los fallos de decode en errores explícitos; el fixture positivo
  Flate y la suite de validación pasan `26/26`, con Clippy estricto.
  `5938897` mapea también los errores y estados de metadata PDF/UA a
  `ISO 14289-1:2014, 7.1`; el crate `pdf-compliance` pasa `8/8` tests y
  Clippy estricto.
  `LANG` ahora distingue valores vacíos de etiquetas mal formadas y acepta
  etiquetas BCP 47 sintácticamente válidas, incluyendo `PdfString` codificado
  en UTF-16; `LANG_INVALID` queda mapeado a `ISO 14289-1:2014, 7.2`, con
  regresiones positivas y negativas.
  `3ad19fe` corrige la lectura de destinos `bfrange` cuyos arrays están
  partidos entre líneas; la suite focalizada de CMap pasa `14/14`, con
  Clippy estricto.
  `c3c6478` evita reportar `FONT_ENCODING_MISSING` en `CIDFont` descendientes,
  cuya codificación pertenece al `Type0` padre; la regresión de fuentes pasa
  `4/4`.
  `ae4f9d1` permite que el modo tolerante recupere offsets de objetos fuera
  del fichero como `Null` diagnosticado, mientras strict conserva el error;
  la regresión de offsets inválidos pasa `1/1`.
  `d65d6c3` rechaza tokens hexadecimales inválidos y arrays sin cierre en
  destinos `bfrange`; la suite de CMap pasa `15/15`.
  `9756abe` exige el marcador `~>` de ASCII85 y rechaza el shorthand `z` en
  una tupla parcial; la suite de filtros pasa `9/9`.
  `4934ed1` exige también el terminador `>` de ASCIIHex; la suite de filtros
  pasa `10/10`.
  `ad096ce` hace que la inspección de XMP PDF/UA use el `ResourceBudget` del
  documento; el caso con presupuesto de decode agotado produce
  `METADATA_DECODE_FAILED`, y la suite focalizada pasa `3/3`.
  `4a606ad` valida orden, tamaño y correspondencia de destinos en arrays
  `bfrange`, evitando mapeos parciales; la suite de CMap se mantiene en
  `15/15`.
  `97cd5b7` reutiliza el parser presupuestado de offsets al extraer streams de
  objetos lazy y selecciona el siguiente offset mayor, incluso con entradas
  fuera de orden; la suite focalizada pasa `7/7` y Clippy estricto.
  `75e4a1e` elimina de la recuperación básica la fabricación de xref y
  `/Root 1 0 R`, dejando un diagnóstico sin metadata inventada; la suite de
  recovery pasa `9/9` y Clippy estricto.
  `7fe9afd` rechaza `/Length` indirecto en el parser standalone cuando no se
  ha resuelto, en lugar de buscar un `endstream` ambiguo; `object_parser` pasa
  `7/7`, `parser_tests` `32/32` y Clippy estricto.
  `9fe85c2` corrige el rebuild de xref para registrar el offset exacto del
  encabezado, calcular `/Size` con el mayor objeto y conservar `/Root` solo si
  ya estaba declarado; recovery pasa `9/9` y Clippy estricto.
  `5babb79` hace byte-based la reparación de streams Flate, evitando que
  `from_utf8_lossy` desplace índices sobre datos binarios; la suite de recovery
  pasa `10/10` y Clippy estricto.
  `a0fed6c` acota el fallback del header truncado en reconstrucción para evitar
  slices fuera de rango; la regresión pasa `1/1` y Clippy estricto.
  La cobertura reproducible añade `LANG_INVALID` con el par upstream
  `7.2-t24-pass-a`/`7.2-t29-fail-b`; la prueba local y la correspondencia
  `ISO 14289-1:2014:7.2:29` de veraPDF pasan.
  `6b2372d` reinicia los contadores de `ResourceBudget` entre operaciones
  independientes de `PdfParser`, conservando el presupuesto compartido dentro
  de cada documento; parser focalizado `9/9`, `parser_tests` `32/32` y Clippy
  estricto pasan.
* **Conformidad:** ya existe el inventario publico por clausula en
  `ISO-32000-MATRIX.md` y el mapeo reproducible de veraPDF, pero falta
  convertirlo en cobertura normativa completa y dejar PDF/A/PDF/UA fuera de
  estado experimental. PDF/A-1b ya comparte ahora la colección de constraints
  del registro con el validador publico.
  La regla PDF/UA de metadata ahora exige estructuralmente `/Type /Metadata`
  y `/Subtype /XML`, con par sintetico y el par real `7.1-t08` del corpus
  verificado contra `ISO 14289-1:2014:7.1:8`; esto no cambia el estado
  experimental del conjunto.
* **Caracteristicas parciales:** CMap, `ToUnicode`, fuentes, herencia de
  recursos, revisiones hibridas e incrementalmente complejas y codecs aun
  necesitan cobertura de casos limite. JBIG2 tiene decodificacion acotada,
  incluida la resolucion de `JBIG2Globals`, y JPX tiene decodificacion de
  pixeles acotada; ninguno constituye soporte completo del estandar.
* **API y distribucion:** el esquema 1.1.0, visitors, C ABI y chequeo local de
  SemVer existen, pero la API/crates no estan estabilizados ni reorganizados
  para beta. No hay publicación firmada en crates.io, PyPI o npm, ni
  consumidores externos.

El chequeo local de `cargo-semver-checks` pasa para `pdf-ast` contra `0.1.0`,
también invocado sin `--package` sobre este workspace. La
publicacion sigue bloqueada hasta disponer de credenciales de registro,
firma/provenance y artefactos verdes; no se debe crear un release ficticio.

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

Quedan estos puntos de gobernanza:

* `LICENSE` indica MIT.
* El crate compatible se llama `pdf-ast` y el repositorio `pdf-core`; la
  compatibilidad de nombres y las referencias internas a `PDF-AST` están
  documentadas en `COMPATIBILITY.md`.
* No hay GitHub Releases publicadas.
* `main` ya está protegida con los checks de CI, corpus, diferencial, fuzzing,
  API y bindings; falta obtener ejecuciones verdes sostenidas.
* El workspace raíz ya incluye los crates auxiliares y bindings; `fuzz` sigue
  excluido porque usa su propio workspace.
* Las alertas históricas de Dependabot para `rustls-webpki` y `quinn-proto` constan
  como corregidas; `07a7ac0` actualiza `chacha20` fuera de la versión yanked y
  `cargo audit --deny warnings` pasa.

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
