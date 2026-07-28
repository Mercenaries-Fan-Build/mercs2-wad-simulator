inherit("MrxSupport")
import("MrxSupportDesignatorBeacon")

function Create(self, uPlayerGuid)
  local oNewSupport = {}
  setmetatable(oNewSupport, self)
  self.__index = self
  local oDesignator = MrxSupportDesignatorBeacon:Create()
  oNewSupport:SetDesignator(oDesignator)
  oNewSupport:SetOwner(uPlayerGuid)
  oNewSupport:SetRecruit("Pilot")
  oNewSupport:SetModuleName("MrxSmartBomb")
  oNewSupport.sBomb = "Laser Guided Bomb Projectile"
  oNewSupport.uBomb = Pg.GetGuidByName("Laser Guided Bomb Projectile")
  return oNewSupport
end

function DesignationCallback(self)
  local nAngle = math.randi(360)
  local nSpawnX, nSpawnY, nSpawnZ = Pg.FindPointFromCamera(300, 25, -1, self.uOwner, nAngle)
  local nTargetX, nTargetY, nTargetZ = Pg.FindPointFromCamera(100, 80, -1, self.uOwner, nAngle)
  self.uJet = Airstrike.Flyby(self.uDeliveryVehicle, nSpawnX, nSpawnZ, nTargetX, nTargetZ, nTargetY + 150, 50, DropBomb, {self})
  MrxSupport.PlayAirstrikeVO(self.uJet, {
    "Misha-None-Freeplay-Support-12",
    "Misha-None-Freeplay-Support-20",
    "Misha-None-Freeplay-Support-21",
    "Misha-None-Freeplay-Support-01",
    "Misha-None-Freeplay-Support-07",
    "Misha-None-Freeplay-Support-10",
    "Misha-None-Freeplay-Support-23",
    "Misha-None-Freeplay-Support-27",
    "Misha-None-Freeplay-Support-28"
  })
end

function DropBomb(self)
  local nTargetX, nTargetY, nTargetZ, uGuid, uTarget = self:GetDesignator():GetTarget()
  local nSpawnX, nSpawnY, nSpawnZ = Object.GetPosition(self.uJet)
  local nVectorX, nVectorY, nVectorZ = nTargetX - nSpawnX, nTargetY - nSpawnY, nTargetZ - nSpawnZ
  nVectorX, nVectorY, nVectorZ = Math.Normalize(nVectorX, nVectorY, nVectorZ)
  self.uSpawnedBomb = Airstrike.SpawnTargettedOrdnance("Smart Bomb Projectile", nSpawnX, nSpawnY, nSpawnZ, nVectorX, nVectorY, nVectorZ, uGuid, "impact", uTarget, self.uOwner, BombExplodes, {self})
  BlipAircraft(self.uSpawnedBomb, {
    255,
    0,
    0
  })
end

function BombExplodes(self)
  Debug.Printf("BOOOOOOOOOOOOOOOOOOOOOOM")
  local nTargetX, nTargetY, nTargetZ, uGuid, uTarget = self:GetDesignator():GetTarget()
  if uGuid and Player.IsLocal(self.uOwner) then
    Object.Remove(uGuid)
  end
end
