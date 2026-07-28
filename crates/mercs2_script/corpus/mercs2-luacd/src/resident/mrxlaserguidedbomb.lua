inherit("MrxSupport")
import("MrxSupportDesignatorLaser")
import("MrxUtil")
sBomb = "Laser Guided Bomb Projectile"
uBomb = Pg.GetGuidByName("Laser Guided Bomb Projectile")

function Create(self, uPlayerGuid)
  local oNewSupport = {}
  setmetatable(oNewSupport, self)
  self.__index = self
  local oDesignator = MrxSupportDesignatorLaser:Create()
  oNewSupport:SetDesignator(oDesignator)
  oNewSupport:SetOwner(uPlayerGuid)
  oNewSupport:SetRecruit("Pilot")
  oNewSupport:SetModuleName("MrxLaserGuidedBomb")
  oNewSupport.sDeliveryVehicle = self.sDeliveryVehicle
  oNewSupport.uDeliveryVehicle = Pg.GetGuidByName(self.sDeliveryVehicle)
  oNewSupport.sBomb = self.sBomb
  oNewSupport.uBomb = Pg.GetGuidByName(self.sBomb)
  oNewSupport.tVOCues = self.tVOCues
  return oNewSupport
end

function DesignationCallback(self)
  local nSpawnX, nSpawnY, nSpawnZ = Pg.FindPointFromCamera(-300, 100, -1, self.uOwner)
  local nTargetX, nTargetY, nTargetZ = Pg.FindPointFromCamera(-100, 80, -1, self.uOwner)
  local uJet = Airstrike.Flyby(self.uDeliveryVehicle, nSpawnX, nSpawnZ, nTargetX, nTargetZ, nTargetY, 120, DropBomb, {self})
  self.uJet = uJet
  Event.Create(Event.TimerRelative, {2}, MrxSupport.PlayAirstrikeVO, {
    uJet,
    {
      "Misha-None-Freeplay-Support-08",
      "Misha-None-Freeplay-Support-19",
      "Misha-None-Freeplay-Support-21",
      "Misha-None-Freeplay-Support-23",
      "Misha-None-Freeplay-Support-24",
      "Misha-None-Freeplay-Support-30"
    }
  })
end

function DropBomb(self)
  local nSpawnX, nSpawnY, nSpawnZ = Object.GetPosition(self.uJet)
  local nTargetX, nTargetY, nTargetZ, uGuid, uTarget = self:GetDesignator():GetTarget()
  nTargetX = nTargetX + math.randf() * 25 - math.randf() * 25
  nTargetY = nTargetY + math.randf() * 25 - math.randf() * 25
  nTargetZ = nTargetZ + math.randf() * 25 - math.randf() * 25
  local nVectorX, nVectorY, nVectorZ = nTargetX - nSpawnX, nTargetY - nSpawnY, nTargetZ - nSpawnZ
  nVectorX, nVectorY, nVectorZ = Math.Normalize(nVectorX, nVectorY, nVectorZ)
  nTargetX, nTargetY, nTargetZ = Object.GetPosition(uGuid)
  self.uTarget = uGuid
  self.uSpawnedBomb = Airstrike.SpawnTargettedOrdnance(self.uBomb, nSpawnX, nSpawnY, nSpawnZ, nVectorX * 80, nVectorY * 80, nVectorZ * 80, uGuid, "impact", 1, self:GetOwner(), self.BombExplodes, {self})
  BlipAircraft(self.uSpawnedBomb, {
    255,
    0,
    0
  })
end

function BombExplodes(self)
  Event.Create(Event.TimerRelative, {1.5}, CreateDebris, {self})
end

function CreateDebris(self)
  local x, y, z = self:GetDesignator():GetTarget()
  if Net.IsClient() or MrxUtil.GetDistanceToObject(Player.GetLocalCharacter(), x, y, z) > 150 then
    return
  end
  x, y, z = Object.GetPosition(Player.GetLocalCharacter())
  Graphics.Effect.Terrain("global_particle_dustfall", 20, 0.005, x, 8, z)
end
