inherit("MrxSupport")
import("MrxSupportDesignatorSmoke")
local sProjectileName = "Bomb"
local sExplosionName = "Explosion (Bombing Run)"

function Create(self, uPlayerGuid)
  local oNewSupport = {}
  setmetatable(oNewSupport, self)
  self.__index = self
  local oDesignator = MrxSupportDesignatorSmoke:Create()
  oDesignator:SetAATestLevel("basic")
  oDesignator:SetValidationFunction(_NoValidation)
  oNewSupport:SetDesignator(oDesignator)
  oNewSupport:SetOwner(uPlayerGuid)
  oNewSupport:SetRecruit("Pilot")
  oNewSupport:SetModuleName("MrxBombingRun")
  oNewSupport.sDeliveryVehicle = self.sDeliveryVehicle
  oNewSupport.uDeliveryVehicle = Pg.GetGuidByName(self.sDeliveryVehicle)
  return oNewSupport
end

function DesignationCallback(self)
  local nSpawnX, nSpawnY, nSpawnZ = Pg.FindPointFromCamera(-300, 100, -1, self.uOwner)
  local nTargetX, nTargetY, nTargetZ = Pg.FindPointFromCamera(0, 100, -1, self.uOwner)
  local uJet = Airstrike.Flyby(self.uDeliveryVehicle, nSpawnX, nSpawnZ, nTargetX, nTargetZ, nTargetY, 200, DropBomb, {self})
  self.uJet = uJet
  MrxSupport.PlayAirstrikeVO(uJet, {
    "Misha-None-Freeplay-Support-01",
    "Misha-None-Freeplay-Support-19",
    "Misha-None-Freeplay-Support-23",
    "Misha-None-Freeplay-Support-32"
  })
end

function DropBomb(self)
  Debug.Printf("DropBomb")
  local nSpawnX, nSpawnY, nSpawnZ = Object.GetPosition(self.uJet)
  local nTargetX, nTargetY, nTargetZ, uGuid, uTarget = self:GetDesignator():GetTarget()
  local nVectorX, nVectorY, nVectorZ = nTargetX - nSpawnX, nTargetY - nSpawnY, nTargetZ - nSpawnZ
  nVectorX, nVectorY, nVectorZ = Math.Normalize(nVectorX, nVectorY, nVectorZ)
  local nSpeedScale = 33
  self.uSpawnedBomb = Airstrike.SpawnOrdnance(sProjectileName, nSpawnX, nSpawnY, nSpawnZ, nVectorX * nSpeedScale, nVectorY * nSpeedScale, nVectorZ * nSpeedScale, uTarget, "impact", nil, self:GetOwner(), BombExplodes, {self})
  self.uSpawnedBomb = Airstrike.SpawnOrdnance(sProjectileName, nSpawnX + 3, nSpawnY + 4, nSpawnZ + 3, nVectorX * nSpeedScale - 2, nVectorY * nSpeedScale - 2, nVectorZ * nSpeedScale - 2, uTarget, "impact", nil, self:GetOwner(), BombExplodes, {self})
end

function BombExplodes(self, uBomb)
end
