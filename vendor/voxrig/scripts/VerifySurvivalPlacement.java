// Original oracle caller; no native game method bodies are redistributed.
import com.google.gson.*;
import io.netty.buffer.Unpooled;
import java.nio.file.*;
import java.util.*;
import net.minecraft.Bootstrap;
import net.minecraft.SharedConstants;
import net.minecraft.block.Block;
import net.minecraft.block.AbstractBlock;
import net.minecraft.item.BlockItem;
import net.minecraft.network.PacketByteBuf;
import net.minecraft.network.packet.c2s.play.PlayerInteractBlockC2SPacket;
import net.minecraft.registry.Registries;
import net.minecraft.util.Hand;
import net.minecraft.util.Identifier;
import net.minecraft.util.hit.BlockHitResult;
import net.minecraft.util.math.*;
public final class VerifySurvivalPlacement {
    public static void main(String[] args) throws Exception {
        SharedConstants.createGameVersion();
        if (!SharedConstants.getGameVersion().name().equals("1.21.11")) throw new IllegalStateException("wrong version");
        Bootstrap.initialize();
        var out = new JsonObject(); var materials = new JsonArray();
        for (var name : List.of("stone","dirt","cobblestone","oak_planks","spruce_planks","quartz_block","smooth_quartz","white_concrete","glass","andesite","granite")) {
            var block = Registries.BLOCK.get(Identifier.ofVanilla(name));
            var item = block.asItem();
            if (item.getClass() != BlockItem.class || ((BlockItem)item).getBlock() != block
                || block.getStateManager().getStates().size() != 1 || !block.getDefaultState().getEntries().isEmpty())
                throw new IllegalStateException("not a plain one-state BlockItem: " + name);
            var owners = new JsonObject();
            for (var method : List.of("getPlacementState","onPlaced","onUse","onUseWithItem","getStateForNeighborUpdate","canPlaceAt","canReplace")) {
                var found = Arrays.stream(block.getClass().getMethods()).filter(m -> m.getName().equals(method)).toList();
                if (found.size() != 1) throw new IllegalStateException("ambiguous method " + method);
                var owner = found.get(0).getDeclaringClass();
                if (owner != Block.class && owner != AbstractBlock.class) throw new IllegalStateException("special placement behavior " + name + " " + method + " " + owner);
                owners.addProperty(method,owner.getName());
            }
            var entry = new JsonObject(); entry.addProperty("name","minecraft:"+name);
            entry.addProperty("item_id",Registries.ITEM.getRawId(item));
            entry.addProperty("state_id",Block.getRawIdFromState(block.getDefaultState()));
            entry.add("method_owners",owners); materials.add(entry);
        }
        out.add("materials",materials); var packets = new JsonArray();
        for (var face : Direction.values()) {
            var pos = new BlockPos(-2,-61,3);
            var hit = new Vec3d(pos.getX()+0.5+face.getOffsetX()*0.5,pos.getY()+0.5+face.getOffsetY()*0.5,pos.getZ()+0.5+face.getOffsetZ()*0.5);
            var buffer = new PacketByteBuf(Unpooled.buffer());
            try {
                var packet = new PlayerInteractBlockC2SPacket(Hand.MAIN_HAND,new BlockHitResult(hit,face,pos,false),17);
                PlayerInteractBlockC2SPacket.CODEC.encode(buffer,packet);
                var bytes = new byte[buffer.readableBytes()]; buffer.getBytes(buffer.readerIndex(),bytes);
                var decoded = PlayerInteractBlockC2SPacket.CODEC.decode(buffer);
                if (buffer.isReadable() || decoded.getSequence()!=17 || decoded.getHand()!=Hand.MAIN_HAND || !decoded.getBlockHitResult().getBlockPos().equals(pos) || decoded.getBlockHitResult().getSide()!=face) throw new IllegalStateException("codec mismatch");
                var entry = new JsonObject(); entry.addProperty("face",face.getIndex()); entry.addProperty("hex",HexFormat.of().formatHex(bytes)); packets.add(entry);
            } finally { buffer.release(); }
        }
        out.add("packets",packets);
        Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(out)+"\n");
        System.out.println("11 passive materials and 6 native placement packets verified");
    }
}
