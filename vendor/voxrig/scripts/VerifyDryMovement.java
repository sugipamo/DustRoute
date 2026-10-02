// Calls native 1.21.11 input/rotation and voxel collision methods unchanged.
import com.google.gson.*;
import io.netty.buffer.Unpooled;
import net.minecraft.network.PacketByteBuf;
import net.minecraft.network.packet.c2s.play.PlayerInputC2SPacket;
import net.minecraft.network.packet.c2s.play.PlayerMoveC2SPacket;
import net.minecraft.util.PlayerInput;
import java.nio.file.*;
import java.util.*;
import net.minecraft.Bootstrap;
import net.minecraft.SharedConstants;
import net.minecraft.client.network.ClientPlayerEntity;
import net.minecraft.entity.Entity;
import net.minecraft.block.Blocks;
import net.minecraft.registry.Registries;
import net.minecraft.entity.PlayerLikeEntity;
import net.minecraft.util.math.*;
import net.minecraft.util.shape.*;
public final class VerifyDryMovement {
    static JsonArray array(double... xs) { var a=new JsonArray();for(var x:xs)a.add(x);return a; }
    static JsonArray vec(Vec3d v) { return array(v.x,v.y,v.z); }
    public static void main(String[] args) throws Exception {
        SharedConstants.createGameVersion();Bootstrap.initialize();
        if(!SharedConstants.getGameVersion().name().equals("1.21.11"))throw new IllegalStateException("version");
        var directional=ClientPlayerEntity.class.getDeclaredMethod("applyDirectionalMovementSpeedFactors",Vec2f.class);directional.setAccessible(true);
        var collide=Entity.class.getDeclaredMethod("adjustMovementForCollisions",Vec3d.class,Box.class,List.class);collide.setAccessible(true);
        var inputs=new JsonArray();
        for(int strafe=-1;strafe<=1;strafe++)for(int forward=-1;forward<=1;forward++)for(float yaw:new float[]{0,90,-90,180,35.57f,-405.123f}) {
            var raw=new Vec2f(strafe,forward).normalize().multiply(.98f);
            var input=(Vec2f)directional.invoke(null,raw);
            for(float speed:new float[]{.1f*(.21600002f/(.6f*.6f*.6f)),.02f}) {
                var result=Entity.movementInputToVelocity(new Vec3d(input.x,0,input.y),speed,yaw);
                var c=new JsonObject(); c.addProperty("strafe",strafe);c.addProperty("forward",forward);c.addProperty("yaw",yaw);c.addProperty("speed",speed);c.add("expected",vec(result));inputs.add(c);
            }
        }
        var collisions=new JsonArray();
        var sets=List.of(List.of(new Box(0,0,0,1,1,1)),List.of(new Box(0,0,0,1,1,1),new Box(1,1,0,2,2,1)),List.of(new Box(0,0,0,1,1,1),new Box(0,2,0,1,3,1)),List.of(new Box(1,1,0,2,2,1),new Box(0,1,1,1,2,2)));
        for(var boxes:sets)for(double[] pos:List.of(new double[]{.5,1,.5},new double[]{.5,1.2,.5},new double[]{1.29999999,1,.5}))for(var move:List.of(new Vec3d(.4,-.08,.2),new Vec3d(.2,-.08,.4),new Vec3d(0,.42,0),new Vec3d(0,-.4,0),new Vec3d(-.4,0,-.4),new Vec3d(1e-8,-1e-8,0))) {
            var body=PlayerLikeEntity.STANDING_DIMENSIONS.getBoxAt(new Vec3d(pos[0],pos[1],pos[2]));
            var shapes=boxes.stream().map(VoxelShapes::cuboid).toList();
            var adjusted=(Vec3d)collide.invoke(null,move,body,shapes);
            var c=new JsonObject();c.add("position",array(pos));c.add("motion",vec(move));c.add("expected",vec(adjusted));
            var geometry=new JsonArray();for(var b:boxes)geometry.add(array(b.minX,b.minY,b.minZ,b.maxX,b.maxY,b.maxZ));c.add("boxes",geometry);collisions.add(c);
        }
        var result=new JsonObject();result.add("inputs",inputs);result.add("collisions",collisions);
        var materials=new JsonObject();
        for(var block:List.of(Blocks.STONE,Blocks.DIRT,Blocks.GRASS_BLOCK,Blocks.COBBLESTONE,Blocks.OAK_PLANKS,Blocks.SPRUCE_PLANKS,Blocks.QUARTZ_BLOCK,Blocks.SMOOTH_QUARTZ,Blocks.WHITE_CONCRETE,Blocks.GLASS,Blocks.ANDESITE,Blocks.GRANITE))
            materials.add(Registries.BLOCK.getId(block).toString(),array(block.getSlipperiness(),block.getVelocityMultiplier(),block.getJumpVelocityMultiplier()));
        result.add("materials",materials);
        var packets=new JsonArray();
        for(int strafe=-1;strafe<=1;strafe++)for(int forward=-1;forward<=1;forward++)for(boolean jump:new boolean[]{false,true}) {
            var input=new PlayerInput(forward>0,forward<0,strafe>0,strafe<0,jump,false,false);
            var buffer=new PacketByteBuf(Unpooled.buffer());
            try { PlayerInputC2SPacket.CODEC.encode(buffer,new PlayerInputC2SPacket(input));
                var c=new JsonObject();c.addProperty("strafe",strafe);c.addProperty("forward",forward);c.addProperty("jump",jump);c.addProperty("bits",buffer.readUnsignedByte());if(buffer.isReadable())throw new IllegalStateException();packets.add(c);
            } finally {buffer.release();}
        }
        result.add("input_packets",packets);
        var full=new JsonArray();
        for(boolean ground:new boolean[]{false,true})for(boolean collision:new boolean[]{false,true}) {
            var buffer=new PacketByteBuf(Unpooled.buffer());
            try {PlayerMoveC2SPacket.Full.CODEC.encode(buffer,new PlayerMoveC2SPacket.Full(.5,64.0,-2.5,35.57f,-22.12f,ground,collision));
                var bytes=new byte[buffer.readableBytes()];buffer.readBytes(bytes);var c=new JsonObject();c.addProperty("ground",ground);c.addProperty("collision",collision);c.addProperty("hex",HexFormat.of().formatHex(bytes));full.add(c);
            }finally{buffer.release();}
        }
        result.add("position_packets",full);
        Files.writeString(Path.of(args[0]),new Gson().toJson(result)+"\n");
        System.out.println(inputs.size()+" input/rotation and "+collisions.size()+" collision results exported");
    }
}
