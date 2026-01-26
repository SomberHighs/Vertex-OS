/*
 * VERTEX OS - Node Implementation
 */

 #include "node.h"
 #include <stdlib.h>
 #include <string.h>
 #include <stdio.h>
 
 /* ============================================================================
  * SIMPLE SHA-256 IMPLEMENTATION
  * 
  * We include a basic SHA-256 to avoid external dependencies.
  * In production, you'd use a proper crypto library.
  * ============================================================================ */
 
 static const uint32_t K[64] = {
     0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5,
     0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
     0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
     0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
     0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
     0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
     0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
     0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
     0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
     0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
     0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3,
     0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
     0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5,
     0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
     0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
     0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2
 };
 
 #define ROTR(x, n) (((x) >> (n)) | ((x) << (32 - (n))))
 #define CH(x, y, z) (((x) & (y)) ^ (~(x) & (z)))
 #define MAJ(x, y, z) (((x) & (y)) ^ ((x) & (z)) ^ ((y) & (z)))
 #define EP0(x) (ROTR(x, 2) ^ ROTR(x, 13) ^ ROTR(x, 22))
 #define EP1(x) (ROTR(x, 6) ^ ROTR(x, 11) ^ ROTR(x, 25))
 #define SIG0(x) (ROTR(x, 7) ^ ROTR(x, 18) ^ ((x) >> 3))
 #define SIG1(x) (ROTR(x, 17) ^ ROTR(x, 19) ^ ((x) >> 10))
 
 typedef struct {
     uint8_t data[64];
     uint32_t datalen;
     uint64_t bitlen;
     uint32_t state[8];
 } SHA256_CTX;
 
 static void sha256_transform(SHA256_CTX *ctx, const uint8_t data[]) {
     uint32_t a, b, c, d, e, f, g, h, i, j, t1, t2, m[64];
 
     for (i = 0, j = 0; i < 16; ++i, j += 4)
         m[i] = ((uint32_t)data[j] << 24) | ((uint32_t)data[j+1] << 16) |
                ((uint32_t)data[j+2] << 8) | ((uint32_t)data[j+3]);
     for (; i < 64; ++i)
         m[i] = SIG1(m[i-2]) + m[i-7] + SIG0(m[i-15]) + m[i-16];
 
     a = ctx->state[0]; b = ctx->state[1]; c = ctx->state[2]; d = ctx->state[3];
     e = ctx->state[4]; f = ctx->state[5]; g = ctx->state[6]; h = ctx->state[7];
 
     for (i = 0; i < 64; ++i) {
         t1 = h + EP1(e) + CH(e, f, g) + K[i] + m[i];
         t2 = EP0(a) + MAJ(a, b, c);
         h = g; g = f; f = e; e = d + t1;
         d = c; c = b; b = a; a = t1 + t2;
     }
 
     ctx->state[0] += a; ctx->state[1] += b; ctx->state[2] += c; ctx->state[3] += d;
     ctx->state[4] += e; ctx->state[5] += f; ctx->state[6] += g; ctx->state[7] += h;
 }
 
 static void sha256_init(SHA256_CTX *ctx) {
     ctx->datalen = 0;
     ctx->bitlen = 0;
     ctx->state[0] = 0x6a09e667; ctx->state[1] = 0xbb67ae85;
     ctx->state[2] = 0x3c6ef372; ctx->state[3] = 0xa54ff53a;
     ctx->state[4] = 0x510e527f; ctx->state[5] = 0x9b05688c;
     ctx->state[6] = 0x1f83d9ab; ctx->state[7] = 0x5be0cd19;
 }
 
 static void sha256_update(SHA256_CTX *ctx, const uint8_t data[], size_t len) {
     for (size_t i = 0; i < len; ++i) {
         ctx->data[ctx->datalen] = data[i];
         ctx->datalen++;
         if (ctx->datalen == 64) {
             sha256_transform(ctx, ctx->data);
             ctx->bitlen += 512;
             ctx->datalen = 0;
         }
     }
 }
 
 static void sha256_final(SHA256_CTX *ctx, uint8_t hash[]) {
     uint32_t i = ctx->datalen;
 
     if (ctx->datalen < 56) {
         ctx->data[i++] = 0x80;
         while (i < 56) ctx->data[i++] = 0x00;
     } else {
         ctx->data[i++] = 0x80;
         while (i < 64) ctx->data[i++] = 0x00;
         sha256_transform(ctx, ctx->data);
         memset(ctx->data, 0, 56);
     }
 
     ctx->bitlen += ctx->datalen * 8;
     ctx->data[63] = ctx->bitlen; ctx->data[62] = ctx->bitlen >> 8;
     ctx->data[61] = ctx->bitlen >> 16; ctx->data[60] = ctx->bitlen >> 24;
     ctx->data[59] = ctx->bitlen >> 32; ctx->data[58] = ctx->bitlen >> 40;
     ctx->data[57] = ctx->bitlen >> 48; ctx->data[56] = ctx->bitlen >> 56;
     sha256_transform(ctx, ctx->data);
 
     for (i = 0; i < 4; ++i) {
         hash[i]      = (ctx->state[0] >> (24 - i * 8)) & 0xff;
         hash[i + 4]  = (ctx->state[1] >> (24 - i * 8)) & 0xff;
         hash[i + 8]  = (ctx->state[2] >> (24 - i * 8)) & 0xff;
         hash[i + 12] = (ctx->state[3] >> (24 - i * 8)) & 0xff;
         hash[i + 16] = (ctx->state[4] >> (24 - i * 8)) & 0xff;
         hash[i + 20] = (ctx->state[5] >> (24 - i * 8)) & 0xff;
         hash[i + 24] = (ctx->state[6] >> (24 - i * 8)) & 0xff;
         hash[i + 28] = (ctx->state[7] >> (24 - i * 8)) & 0xff;
     }
 }
 
 /* ============================================================================
  * CONSTANTS
  * ============================================================================ */
 
 const vertex_id VERTEX_NULL_ID = {{0}};
 
 static const char* TYPE_NAMES[] = {
     "blob", "text", "document", "directory",
     "user", "process", "capability", "schema", "custom"
 };
 
 
 /* ============================================================================
  * VERTEX ID OPERATIONS
  * ============================================================================ */
 
 int vertex_id_compare(const vertex_id *a, const vertex_id *b) {
     return memcmp(a->bytes, b->bytes, VERTEX_HASH_SIZE);
 }
 
 bool vertex_id_is_null(const vertex_id *id) {
     return vertex_id_compare(id, &VERTEX_NULL_ID) == 0;
 }
 
 void vertex_id_to_hex(const vertex_id *id, char *out) {
     for (int i = 0; i < VERTEX_HASH_SIZE; i++) {
         sprintf(out + (i * 2), "%02x", id->bytes[i]);
     }
     out[VERTEX_HASH_SIZE * 2] = '\0';
 }
 
 bool vertex_id_from_hex(const char *hex, vertex_id *out) {
     if (strlen(hex) != VERTEX_HASH_SIZE * 2) return false;
     
     for (int i = 0; i < VERTEX_HASH_SIZE; i++) {
         unsigned int byte;
         if (sscanf(hex + (i * 2), "%2x", &byte) != 1) return false;
         out->bytes[i] = (uint8_t)byte;
     }
     return true;
 }
 
 const char* vertex_type_name(vertex_type type) {
     if (type >= 0 && type <= VTYPE_CUSTOM) return TYPE_NAMES[type];
     return "unknown";
 }
 
 
 /* ============================================================================
  * NODE BUILDER
  * ============================================================================ */
 
 struct vertex_node_builder {
     vertex_type  type;
     time_t       created;
     vertex_id    previous;
     
     uint8_t     *data;
     size_t       data_size;
     
     vertex_property *properties;
     size_t           property_count;
     size_t           property_capacity;
     
     vertex_edge *edges;
     size_t       edge_count;
     size_t       edge_capacity;
 };
 
 vertex_node_builder* vertex_node_builder_new(vertex_type type) {
     vertex_node_builder *b = calloc(1, sizeof(vertex_node_builder));
     if (!b) return NULL;
     
     b->type = type;
     b->created = time(NULL);
     b->previous = VERTEX_NULL_ID;
     
     b->property_capacity = 8;
     b->properties = malloc(b->property_capacity * sizeof(vertex_property));
     
     b->edge_capacity = 8;
     b->edges = malloc(b->edge_capacity * sizeof(vertex_edge));
     
     return b;
 }
 
 void vertex_node_builder_free(vertex_node_builder *b) {
     if (!b) return;
     free(b->data);
     free(b->properties);
     free(b->edges);
     free(b);
 }
 
 void vertex_builder_set_data(vertex_node_builder *b, const uint8_t *data, size_t size) {
     free(b->data);
     b->data = malloc(size);
     memcpy(b->data, data, size);
     b->data_size = size;
 }
 
 void vertex_builder_set_text(vertex_node_builder *b, const char *text) {
     vertex_builder_set_data(b, (const uint8_t*)text, strlen(text) + 1);
 }
 
 void vertex_builder_set_previous(vertex_node_builder *b, const vertex_id *prev) {
     b->previous = *prev;
 }
 
 static void ensure_property_capacity(vertex_node_builder *b) {
     if (b->property_count >= b->property_capacity) {
         b->property_capacity *= 2;
         b->properties = realloc(b->properties, b->property_capacity * sizeof(vertex_property));
     }
 }
 
 void vertex_builder_add_property_int(vertex_node_builder *b, const char *key, int64_t value) {
     ensure_property_capacity(b);
     vertex_property *p = &b->properties[b->property_count++];
     strncpy(p->key, key, VERTEX_PROP_KEY_MAX - 1);
     p->type = VPROP_INT;
     p->value.i = value;
 }
 
 void vertex_builder_add_property_string(vertex_node_builder *b, const char *key, const char *value) {
     ensure_property_capacity(b);
     vertex_property *p = &b->properties[b->property_count++];
     strncpy(p->key, key, VERTEX_PROP_KEY_MAX - 1);
     p->type = VPROP_STRING;
     strncpy(p->value.s, value, VERTEX_PROP_VALUE_MAX - 1);
 }
 
 void vertex_builder_add_property_bool(vertex_node_builder *b, const char *key, bool value) {
     ensure_property_capacity(b);
     vertex_property *p = &b->properties[b->property_count++];
     strncpy(p->key, key, VERTEX_PROP_KEY_MAX - 1);
     p->type = VPROP_BOOL;
     p->value.b = value;
 }
 
 void vertex_builder_add_property_time(vertex_node_builder *b, const char *key, time_t value) {
     ensure_property_capacity(b);
     vertex_property *p = &b->properties[b->property_count++];
     strncpy(p->key, key, VERTEX_PROP_KEY_MAX - 1);
     p->type = VPROP_TIMESTAMP;
     p->value.t = value;
 }
 
 static void ensure_edge_capacity(vertex_node_builder *b) {
     if (b->edge_count >= b->edge_capacity) {
         b->edge_capacity *= 2;
         b->edges = realloc(b->edges, b->edge_capacity * sizeof(vertex_edge));
     }
 }
 
 void vertex_builder_add_edge(vertex_node_builder *b, const char *label, 
                              const vertex_id *target, uint32_t flags) {
     ensure_edge_capacity(b);
     vertex_edge *e = &b->edges[b->edge_count++];
     strncpy(e->label, label, VERTEX_EDGE_LABEL_MAX - 1);
     e->target = *target;
     e->flags = flags;
 }
 
 
 /* ============================================================================
  * CONTENT HASHING - This is what makes nodes content-addressed!
  * ============================================================================ */
 
 static vertex_id compute_node_hash(vertex_node_builder *b) {
     SHA256_CTX ctx;
     vertex_id result;
     
     sha256_init(&ctx);
     
     sha256_update(&ctx, (uint8_t*)&b->type, sizeof(b->type));
     sha256_update(&ctx, (uint8_t*)&b->created, sizeof(b->created));
     sha256_update(&ctx, (uint8_t*)&b->previous, sizeof(b->previous));
     
     if (b->data && b->data_size > 0) {
         sha256_update(&ctx, (uint8_t*)&b->data_size, sizeof(b->data_size));
         sha256_update(&ctx, b->data, b->data_size);
     }
     
     for (size_t i = 0; i < b->property_count; i++) {
         vertex_property *p = &b->properties[i];
         sha256_update(&ctx, (uint8_t*)p->key, strlen(p->key));
         sha256_update(&ctx, (uint8_t*)&p->type, sizeof(p->type));
         sha256_update(&ctx, (uint8_t*)&p->value, sizeof(p->value));
     }
     
     for (size_t i = 0; i < b->edge_count; i++) {
         vertex_edge *e = &b->edges[i];
         sha256_update(&ctx, (uint8_t*)e->label, strlen(e->label));
         sha256_update(&ctx, (uint8_t*)&e->target, sizeof(e->target));
         sha256_update(&ctx, (uint8_t*)&e->flags, sizeof(e->flags));
     }
     
     sha256_final(&ctx, result.bytes);
     return result;
 }
 
 
 /* ============================================================================
  * BUILD FINAL NODE
  * ============================================================================ */
 
 vertex_node* vertex_node_build(vertex_node_builder *builder) {
     if (!builder) return NULL;
     
     vertex_node *node = calloc(1, sizeof(vertex_node));
     if (!node) return NULL;
     
     node->id = compute_node_hash(builder);
     node->type = builder->type;
     node->created = builder->created;
     node->previous = builder->previous;
     
     if (builder->data_size > 0) {
         node->data = malloc(builder->data_size);
         memcpy(node->data, builder->data, builder->data_size);
         node->data_size = builder->data_size;
     }
     
     if (builder->property_count > 0) {
         node->property_count = builder->property_count;
         node->properties = malloc(node->property_count * sizeof(vertex_property));
         memcpy(node->properties, builder->properties, 
                node->property_count * sizeof(vertex_property));
     }
     
     if (builder->edge_count > 0) {
         node->edge_count = builder->edge_count;
         node->edges = malloc(node->edge_count * sizeof(vertex_edge));
         memcpy(node->edges, builder->edges, node->edge_count * sizeof(vertex_edge));
     }
     
     return node;
 }
 
 void vertex_node_free(vertex_node *node) {
     if (!node) return;
     free(node->data);
     free(node->properties);
     free(node->edges);
     free(node);
 }
 
 
 /* ============================================================================
  * NODE INSPECTION
  * ============================================================================ */
 
 const vertex_property* vertex_node_get_property(const vertex_node *node, const char *key) {
     for (size_t i = 0; i < node->property_count; i++) {
         if (strcmp(node->properties[i].key, key) == 0) {
             return &node->properties[i];
         }
     }
     return NULL;
 }
 
 const vertex_edge* vertex_node_get_edge(const vertex_node *node, const char *label) {
     for (size_t i = 0; i < node->edge_count; i++) {
         if (strcmp(node->edges[i].label, label) == 0) {
             return &node->edges[i];
         }
     }
     return NULL;
 }
 
 size_t vertex_node_count_edges(const vertex_node *node, const char *label) {
     size_t count = 0;
     for (size_t i = 0; i < node->edge_count; i++) {
         if (strcmp(node->edges[i].label, label) == 0) count++;
     }
     return count;
 }