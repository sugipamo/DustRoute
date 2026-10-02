// Original caller of unchanged Minecraft bodies; no game implementations included.
import com.google.gson.*;
import io.netty.buffer.Unpooled;
import java.nio.file.*;
import java.util.*;
import net.minecraft.Bootstrap;
import net.minecraft.SharedConstants;
import net.minecraft.entity.EntityPosition;
import net.minecraft.network.PacketByteBuf;
import net.minecraft.network.packet.s2c.play.EntityPositionS2CPacket;
import net.minecraft.network.packet.s2c.play.PositionFlag;
import net.minecraft.util.math.Vec3d;
public final class VerifyPositionCorrection {
    static JsonArray vector(Vec3d v) { var a=new JsonArray(); a.add(v.x);a.add(v.y);a.add(v.z);return a; }
    static JsonObject pose(EntityPosition p) {
        var o=new JsonObject();o.add("position",vector(p.position()));o.add("velocity",vector(p.deltaMovement()));
        var r=new JsonArray();r.add(p.yaw());r.add(p.pitch());o.add("rotation",r);return o;
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.createGameVersion();Bootstrap.initialize();
        if(!SharedConstants.getGameVersion().name().equals("1.21.11"))throw new IllegalStateException("version");
        var scenarios=new JsonArray();
        for(int scenario=0;scenario<2;scenario++) {
            var before=new EntityPosition(new Vec3d(-1.25,64.125,12.75),new Vec3d(.13,-.08,.27), scenario==0?80f:-405.123f,scenario==0?-40f:35.57f);
            var change=new EntityPosition(new Vec3d(.5,2,-4),new Vec3d(.01,.02,-.03),scenario==0?130f:23.765f,scenario==0?120f:-22.12f);
            var s=new JsonObject();s.add("before",pose(before));s.add("change",pose(change));var cases=new JsonArray();
            for(int bits=0;bits<512;bits++) {
                var flags=PositionFlag.getFlags(bits);var expected=EntityPosition.apply(before,change,flags);
                var c=new JsonObject();c.addProperty("flags",bits);c.add("expected",pose(expected));
                var buffer=new PacketByteBuf(Unpooled.buffer());
                try {
                    EntityPositionS2CPacket.CODEC.encode(buffer,new EntityPositionS2CPacket(42,change,flags,true));
                    var bytes=new byte[buffer.readableBytes()];buffer.getBytes(buffer.readerIndex(),bytes);
                    c.addProperty("hex",HexFormat.of().formatHex(bytes));
                    var decoded=EntityPositionS2CPacket.CODEC.decode(buffer);
                    if(buffer.isReadable()||!decoded.change().equals(change)||!decoded.relatives().equals(flags)||!decoded.onGround())throw new IllegalStateException("round trip");
                } finally {buffer.release();}
                cases.add(c);
            }
            s.add("cases",cases);scenarios.add(s);
        }
        Files.writeString(Path.of(args[0]),new Gson().toJson(scenarios)+"\n");
        System.out.println("1024 native correction resolutions and packet encodings exported");
    }
}
