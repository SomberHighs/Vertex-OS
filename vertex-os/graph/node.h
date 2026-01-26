/*
 * VERTEX OS - Node System
 * 
 * The fundamental unit of storage in Vertex. Everything is a node.
 * Nodes are content-addressed, immutable, and connected by typed edges.
 */

 #ifndef VERTEX_NODE_H
 #define VERTEX_NODE_H
 
 #include <stdint.h>
 #include <stddef.h>
 #include <time.h>
 #include <stdbool.h>
 
 /* ============================================================================
  * CONTENT ADDRESSING
  * 
  * Nodes are identified by the SHA-256 hash of their content.
  * This provides: integrity, deduplication, and immutability.
  * ============================================================================ */
 
 #define VERTEX_HASH_SIZE 32  /* SHA-256 = 256 bits = 32 bytes */
 
 typedef struct {
     uint8_t bytes[VERTEX_HASH_SIZE];
 } vertex_id;
 
 /* Special null ID for invalid/unset references */
 extern const vertex_id VERTEX_NULL_ID;
 
 /* Compare two vertex IDs */
 int vertex_id_compare(const vertex_id *a, const vertex_id *b);
 
 /* Check if ID is null */
 bool vertex_id_is_null(const vertex_id *id);
 
 /* Format ID as hex string (caller provides buffer of 65+ chars) */
 void vertex_id_to_hex(const vertex_id *id, char *out);
 
 /* Parse hex string to ID */
 bool vertex_id_from_hex(const char *hex, vertex_id *out);
 
 
 /* ============================================================================
  * NODE TYPES
  * 
  * Nodes have semantic types that determine their behavior and valid edges.
  * ============================================================================ */
 
 typedef enum {
     VTYPE_BLOB,       /* Raw binary data */
     VTYPE_TEXT,       /* UTF-8 text */
     VTYPE_DOCUMENT,   /* Structured document with metadata */
     VTYPE_DIRECTORY,  /* Collection of edges to other nodes */
     VTYPE_USER,       /* User identity */
     VTYPE_PROCESS,    /* Running process state */
     VTYPE_CAPABILITY, /* Capability token */
     VTYPE_SCHEMA,     /* Type schema definition */
     VTYPE_CUSTOM,     /* User-defined type (references a schema) */
 } vertex_type;
 
 const char* vertex_type_name(vertex_type type);
 
 
 /* ============================================================================
  * EDGES
  * 
  * Edges connect nodes with labeled, typed relationships.
  * Unlike Unix symlinks, edges are first-class queryable objects.
  * ============================================================================ */
 
 #define VERTEX_EDGE_LABEL_MAX 64
 
 typedef struct {
     char      label[VERTEX_EDGE_LABEL_MAX];  /* Relationship name */
     vertex_id target;                         /* Node this edge points to */
     uint32_t  flags;                          /* Edge properties */
 } vertex_edge;
 
 /* Edge flags */
 #define VEDGE_STRONG    0x0001  /* Prevents target from being garbage collected */
 #define VEDGE_ORDERED   0x0002  /* Edge participates in ordered collection */
 #define VEDGE_UNIQUE    0x0004  /* Only one edge with this label allowed */
 
 
 /* ============================================================================
  * PROPERTIES (Key-Value Metadata)
  * ============================================================================ */
 
 #define VERTEX_PROP_KEY_MAX   64
 #define VERTEX_PROP_VALUE_MAX 256
 
 typedef enum {
     VPROP_NULL,
     VPROP_INT,
     VPROP_FLOAT,
     VPROP_STRING,
     VPROP_BOOL,
     VPROP_TIMESTAMP,
     VPROP_ID,      /* Reference to another node */
 } vertex_prop_type;
 
 typedef struct {
     char             key[VERTEX_PROP_KEY_MAX];
     vertex_prop_type type;
     union {
         int64_t      i;
         double       f;
         char         s[VERTEX_PROP_VALUE_MAX];
         bool         b;
         time_t       t;
         vertex_id    id;
     } value;
 } vertex_property;
 
 
 /* ============================================================================
  * THE NODE
  * 
  * The fundamental storage unit of Vertex OS.
  * ============================================================================ */
 
 typedef struct vertex_node {
     /* Identity & Type */
     vertex_id    id;           /* Content-addressed identifier */
     vertex_type  type;         /* Semantic type */
     
     /* Temporal */
     time_t       created;      /* Creation timestamp */
     vertex_id    previous;     /* Previous version (for version chains) */
     
     /* Content */
     uint8_t     *data;         /* Raw content bytes */
     size_t       data_size;    /* Size of content */
     
     /* Metadata */
     vertex_property *properties;    /* Key-value properties */
     size_t           property_count;
     
     /* Relationships */
     vertex_edge *edges;        /* Outgoing edges */
     size_t       edge_count;
     
 } vertex_node;
 
 
 /* ============================================================================
  * NODE BUILDER (for constructing immutable nodes)
  * ============================================================================ */
 
 typedef struct vertex_node_builder vertex_node_builder;
 
 vertex_node_builder* vertex_node_builder_new(vertex_type type);
 void vertex_node_builder_free(vertex_node_builder *builder);
 
 /* Builder methods */
 void vertex_builder_set_data(vertex_node_builder *b, const uint8_t *data, size_t size);
 void vertex_builder_set_text(vertex_node_builder *b, const char *text);
 void vertex_builder_set_previous(vertex_node_builder *b, const vertex_id *prev);
 void vertex_builder_add_property_int(vertex_node_builder *b, const char *key, int64_t value);
 void vertex_builder_add_property_string(vertex_node_builder *b, const char *key, const char *value);
 void vertex_builder_add_property_bool(vertex_node_builder *b, const char *key, bool value);
 void vertex_builder_add_property_time(vertex_node_builder *b, const char *key, time_t value);
 void vertex_builder_add_edge(vertex_node_builder *b, const char *label, const vertex_id *target, uint32_t flags);
 
 /* Finalize and compute content hash - returns immutable node */
 vertex_node* vertex_node_build(vertex_node_builder *builder);
 
 /* Free a node */
 void vertex_node_free(vertex_node *node);
 
 /* Node inspection */
 const vertex_property* vertex_node_get_property(const vertex_node *node, const char *key);
 const vertex_edge* vertex_node_get_edge(const vertex_node *node, const char *label);
 size_t vertex_node_count_edges(const vertex_node *node, const char *label);
 
 
 #endif /* VERTEX_NODE_H */