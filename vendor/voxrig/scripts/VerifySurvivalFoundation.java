// Native Java 1.21.11 oracle; no Minecraft method bodies are redistributed.
import com.google.gson.*;
import io.netty.buffer.Unpooled;
import java.nio.file.*;
import java.util.*;
import net.minecraft.Bootstrap;
import net.minecraft.SharedConstants;
import net.minecraft.entity.PlayerLikeEntity;
import net.minecraft.entity.attribute.ClampedEntityAttribute;
import net.minecraft.entity.attribute.EntityAttributes;
import net.minecraft.entity.player.PlayerEntity;
import net.minecraft.block.Blocks;
import net.minecraft.block.ShapeContext;
import net.minecraft.network.PacketByteBuf;
import net.minecraft.network.encoding.VelocityEncoding;
import net.minecraft.network.packet.c2s.play.PlayerMoveC2SPacket;
import net.minecraft.registry.Registries;
import net.minecraft.world.EmptyBlockView;
import net.minecraft.util.math.*;
import net.minecraft.util.shape.VoxelShapes;

public final class VerifySurvivalFoundation {
    public static void main(String[] args) throws Exception {
        SharedConstants.createGameVersion();
        if (!SharedConstants.getGameVersion().name().equals("1.21.11"))
            throw new IllegalStateException("requires Java 1.21.11");
        Bootstrap.initialize();
        var result = new JsonObject();
        var dimensions = PlayerLikeEntity.STANDING_DIMENSIONS;
        var body = new JsonArray();
        body.add(dimensions.width()); body.add(dimensions.height()); body.add(dimensions.eyeHeight());
        result.add("standing_dimensions", body);
        var cubes = new JsonArray();
        for (var block : List.of(Blocks.STONE, Blocks.DIRT, Blocks.GRASS_BLOCK,
                Blocks.COBBLESTONE, Blocks.OAK_PLANKS, Blocks.SPRUCE_PLANKS,
                Blocks.QUARTZ_BLOCK, Blocks.SMOOTH_QUARTZ, Blocks.WHITE_CONCRETE,
                Blocks.GLASS, Blocks.ANDESITE, Blocks.GRANITE)) {
            for (var state : block.getStateManager().getStates()) {
                var shape = state.getCollisionShape(EmptyBlockView.INSTANCE, BlockPos.ORIGIN,
                    ShapeContext.absent());
                if (!shape.getBoundingBoxes().equals(VoxelShapes.fullCube().getBoundingBoxes())
                        || !state.getFluidState().isEmpty())
                    throw new IllegalStateException("admitted state is not a dry full cube: " + state);
            }
            cubes.add(Registries.BLOCK.getId(block).toString());
        }
        result.add("dry_cubes", cubes);
        var attributes = new JsonArray();
        var defaults = PlayerEntity.createPlayerAttributes().build();
        for (var entry : List.of(EntityAttributes.SCALE, EntityAttributes.BLOCK_BREAK_SPEED,
                EntityAttributes.MINING_EFFICIENCY, EntityAttributes.SUBMERGED_MINING_SPEED)) {
            var a = new JsonObject();
            var definition = (ClampedEntityAttribute) entry.value();
            a.addProperty("id", Registries.ATTRIBUTE.getRawId(definition));
            a.addProperty("name", Registries.ATTRIBUTE.getId(definition).toString());
            a.addProperty("default", defaults.getValue(entry));
            a.addProperty("min", definition.getMinValue()); a.addProperty("max", definition.getMaxValue());
            attributes.add(a);
        }
        result.add("attributes", attributes);
        var velocities = new JsonArray();
        for (var v : List.of(Vec3d.ZERO, new Vec3d(0.1, -0.08, 0.3),
                new Vec3d(-3, 0, 8), new Vec3d(16384, -9000, 0))) {
            var buffer = new PacketByteBuf(Unpooled.buffer());
            try {
                VelocityEncoding.writeVelocity(buffer, v);
                byte[] bytes = new byte[buffer.readableBytes()];
                buffer.getBytes(buffer.readerIndex(), bytes);
                var decoded = VelocityEncoding.readVelocity(buffer);
                if (buffer.isReadable()) throw new IllegalStateException("unread velocity bytes");
                var a = new JsonObject(); a.addProperty("hex", HexFormat.of().formatHex(bytes));
                var components = new JsonArray();
                components.add(decoded.x); components.add(decoded.y); components.add(decoded.z);
                a.add("decoded", components); velocities.add(a);
            } finally { buffer.release(); }
        }
        result.add("velocities", velocities);
        var looks = new JsonArray();
        for (boolean ground : List.of(false, true)) {
            var buffer = new PacketByteBuf(Unpooled.buffer());
            try {
                var packet = new PlayerMoveC2SPacket.LookAndOnGround(45, -20, ground, false);
                PlayerMoveC2SPacket.LookAndOnGround.CODEC.encode(buffer, packet);
                byte[] bytes = new byte[buffer.readableBytes()];
                buffer.getBytes(buffer.readerIndex(), bytes);
                var decoded = PlayerMoveC2SPacket.LookAndOnGround.CODEC.decode(buffer);
                if (buffer.isReadable() || decoded.isOnGround() != ground || decoded.horizontalCollision())
                    throw new IllegalStateException("look codec mismatch");
                var a = new JsonObject(); a.addProperty("ground", ground);
                a.addProperty("hex", HexFormat.of().formatHex(bytes)); looks.add(a);
            } finally { buffer.release(); }
        }
        result.add("looks", looks);
        var contacts = new JsonArray();
        var floor = List.of(VoxelShapes.fullCube());
        for (double[] p : List.of(new double[]{0.5,1,0.5}, new double[]{1.25,1,0.5},
                new double[]{1.5,1,0.5}, new double[]{1.3,1,0.5},
                new double[]{1.29999999,1,0.5}, new double[]{0.5,1.001,0.5},
                new double[]{0.5,0.9,0.5}, new double[]{0.5,1.00000001,0.5})) {
            var box = dimensions.getBoxAt(new Vec3d(p[0],p[1],p[2]));
            boolean clear = !box.intersects(new Box(0,0,0,1,1,1));
            double dy = VoxelShapes.calculateMaxOffset(Direction.Axis.Y, box, floor, -1e-7);
            var a = new JsonObject(); var position = new JsonArray();
            for (double value : p) position.add(value);
            a.add("position",position); a.addProperty("clear",clear);
            a.addProperty("ground",clear && dy > -1e-7); contacts.add(a);
        }
        result.add("contacts", contacts);
        Path output = Path.of(args[0]);
        if (args.length > 1 && args[1].equals("--check")) {
            if (!result.equals(JsonParser.parseString(Files.readString(output))))
                throw new IllegalStateException("foundation fixture differs from native results");
        } else Files.writeString(output, new GsonBuilder().setPrettyPrinting().create().toJson(result)+"\n");
        System.out.println("Java 1.21.11 survival foundation: native dimensions, attributes, velocities, looks and contacts verified");
    }
}
