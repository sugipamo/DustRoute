// Original development-oracle caller; no native game bodies are redistributed.
import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.Bootstrap;
import net.minecraft.SharedConstants;
import net.minecraft.entity.attribute.ClampedEntityAttribute;
import net.minecraft.entity.attribute.EntityAttributes;
import net.minecraft.entity.player.PlayerEntity;
import net.minecraft.registry.Registries;
public final class VerifySurvivalMovement {
    public static void main(String[] args) throws Exception {
        SharedConstants.createGameVersion();
        if (!SharedConstants.getGameVersion().name().equals("1.21.11")) throw new IllegalStateException("wrong version");
        Bootstrap.initialize();
        var defaults=PlayerEntity.createPlayerAttributes().build();
        var attributes=new JsonArray();
        for (var entry : List.of(EntityAttributes.MOVEMENT_SPEED,EntityAttributes.GRAVITY,
                EntityAttributes.JUMP_STRENGTH,EntityAttributes.STEP_HEIGHT,
                EntityAttributes.MOVEMENT_EFFICIENCY,EntityAttributes.SNEAKING_SPEED,
                EntityAttributes.SAFE_FALL_DISTANCE,EntityAttributes.FALL_DAMAGE_MULTIPLIER)) {
            var definition=(ClampedEntityAttribute)entry.value(); var a=new JsonObject();
            a.addProperty("id",Registries.ATTRIBUTE.getRawId(definition));
            a.addProperty("name",Registries.ATTRIBUTE.getId(definition).toString());
            a.addProperty("default",defaults.getValue(entry));
            a.addProperty("min",definition.getMinValue()); a.addProperty("max",definition.getMaxValue());
            a.addProperty("tracked",definition.isTracked()); attributes.add(a);
        }
        var output=new JsonObject(); output.add("attributes",attributes);
        Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(output)+"\n");
        System.out.println("8 native player movement attribute defaults/limits exported");
    }
}
