/*
 * VERTEX OS - Node System Test
 */

 #include <stdio.h>
 #include "../graph/node.h"
 
 int main(void) {
     printf("\n=== VERTEX OS - Node Test ===\n\n");
 
     /* Create a user node */
     printf("Creating user node...\n");
     vertex_node_builder *builder = vertex_node_builder_new(VTYPE_USER);
     vertex_builder_add_property_string(builder, "username", "alice");
     vertex_builder_add_property_string(builder, "email", "alice@vertex.os");
     vertex_builder_add_property_bool(builder, "active", true);
     
     vertex_node *user = vertex_node_build(builder);
     vertex_node_builder_free(builder);
 
     /* Print the node info */
     char id_hex[65];
     vertex_id_to_hex(&user->id, id_hex);
     
     printf("\nNode created!\n");
     printf("  ID:   %s\n", id_hex);
     printf("  Type: %s\n", vertex_type_name(user->type));
     
     /* Read back properties */
     const vertex_property *username = vertex_node_get_property(user, "username");
     const vertex_property *email = vertex_node_get_property(user, "email");
     const vertex_property *active = vertex_node_get_property(user, "active");
     
     printf("\nProperties:\n");
     printf("  username: %s\n", username->value.s);
     printf("  email:    %s\n", email->value.s);
     printf("  active:   %s\n", active->value.b ? "true" : "false");
 
     /* Test content addressing - same content = same hash */
     printf("\n--- Content Addressing Test ---\n");
     
     vertex_node_builder *b2 = vertex_node_builder_new(VTYPE_USER);
     vertex_builder_add_property_string(b2, "username", "alice");
     vertex_builder_add_property_string(b2, "email", "alice@vertex.os");
     vertex_builder_add_property_bool(b2, "active", true);
     
     vertex_node *user2 = vertex_node_build(b2);
     vertex_node_builder_free(b2);
     
     char id2_hex[65];
     vertex_id_to_hex(&user2->id, id2_hex);
     
     printf("Same content, new node:\n");
     printf("  ID 1: %.16s...\n", id_hex);
     printf("  ID 2: %.16s...\n", id2_hex);
     printf("  Match: %s\n", vertex_id_compare(&user->id, &user2->id) == 0 ? "YES!" : "no");
 
     /* Cleanup */
     vertex_node_free(user);
     vertex_node_free(user2);
 
     printf("\n=== Test Complete ===\n\n");
     return 0;
 }