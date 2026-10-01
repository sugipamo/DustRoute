// Native Java 1.21.11 development oracle. Minecraft JARs/code are not redistributed.
// Verifies the component-free SWAP packet used without optimistic modified hashes.
import com.google.gson.*;
import io.netty.buffer.Unpooled;
import it.unimi.dsi.fastutil.ints.Int2ObjectMaps;
import java.nio.file.*;
import java.util.*;
import net.minecraft.Bootstrap;
import net.minecraft.SharedConstants;
import net.minecraft.network.RegistryByteBuf;
import net.minecraft.network.packet.c2s.play.ClickSlotC2SPacket;
import net.minecraft.registry.DynamicRegistryManager;
import net.minecraft.screen.slot.SlotActionType;
import net.minecraft.screen.sync.ItemStackHash;

public final class VerifyInventoryClick {
    public static void main(String[] args) throws Exception {
        SharedConstants.createGameVersion();
        if (!SharedConstants.getGameVersion().name().equals("1.21.11"))
            throw new IllegalStateException("requires Java 1.21.11");
        Bootstrap.initialize();
        var cases = JsonParser.parseString(Files.readString(Path.of(args[0]))).getAsJsonArray();
        for (var entry : cases) {
            var c = entry.getAsJsonObject();
            int revision = c.get("revision").getAsInt();
            short slot = c.get("main_slot").getAsShort();
            byte hotbar = c.get("hotbar").getAsByte();
            var expected = HexFormat.of().parseHex(c.get("payload_hex").getAsString());
            var buffer = new RegistryByteBuf(Unpooled.buffer(), DynamicRegistryManager.EMPTY);
            try {
                var packet = new ClickSlotC2SPacket(0, revision, slot, hotbar,
                    SlotActionType.SWAP, Int2ObjectMaps.emptyMap(), ItemStackHash.EMPTY);
                ClickSlotC2SPacket.CODEC.encode(buffer, packet);
                byte[] actual = new byte[buffer.readableBytes()];
                buffer.getBytes(buffer.readerIndex(), actual);
                if (!Arrays.equals(expected, actual))
                    throw new IllegalStateException("native SWAP encoding mismatch: " + c);
                var decoded = ClickSlotC2SPacket.CODEC.decode(buffer);
                if (buffer.isReadable() || decoded.syncId() != 0 || decoded.revision() != revision
                    || decoded.slot() != slot || decoded.button() != hotbar
                    || decoded.actionType() != SlotActionType.SWAP
                    || !decoded.modifiedStacks().isEmpty() || decoded.cursor() != ItemStackHash.EMPTY)
                    throw new IllegalStateException("native SWAP decoding mismatch: " + c);
            } finally { buffer.release(); }
        }
        System.out.println("Java 1.21.11 native SWAP codec: " + cases.size() + " cases verified");
    }
}
