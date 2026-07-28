inherit("MrxSupport")
import("MrxSupportDesignatorSmoke")

function Create(self, uPlayerGuid)
  local oNewSupport = {}
  setmetatable(oNewSupport, self)
  self.__index = self
  local oDesignator = MrxSupportDesignatorSmoke:Create()
  oNewSupport:SetDesignator(oDesignator)
  oDesignator:SetSmokeColor("red")
  oDesignator:SetAATestLevel("basic")
  oDesignator:SetValidationFunction(_NoValidation)
  oNewSupport:SetOwner(uPlayerGuid)
  oNewSupport:SetRecruit("Pilot")
  oNewSupport:SetDesignator(oDesignator)
  oNewSupport:SetModuleName("MrxClusterBomb")
  oNewSupport.sDeliveryVehicle = self.sDeliveryVehicle
  oNewSupport.uDeliveryVehicle = Pg.GetGuidByName(self.sDeliveryVehicle)
  return oNewSupport
end

function DesignationCallback(self)
  MrxSupportDesignatorSmoke:DesignationCompleteCallback()
  local nAngle = 300 + math.randi(120)
  local nSpawnX, nSpawnY, nSpawnZ = Pg.FindPointFromCamera(300, 200, -1, self.uOwner, nAngle)
  local nTargetX, nTargetY, nTargetZ = Pg.FindPointFromCamera(100, 50, -1, self.uOwner, nAngle)
  local uJet = Airstrike.Flyby(self.uDeliveryVehicle, nSpawnX, nSpawnZ, nTargetX, nTargetZ, nTargetY, 100, DropBomb, {self})
  self.uJet = uJet
  Event.Create(Event.TimerRelative, {2}, MrxSupport.PlayAirstrikeVO, {
    uJet,
    {
      "Misha-None-Freeplay-Support-03",
      "Misha-None-Freeplay-Support-14",
      "Misha-None-Freeplay-Support-25"
    }
  })
end

function DropBomb(self)
  local nSpawnX, nSpawnY, nSpawnZ = Object.GetPosition(self.uJet)
  local nTargetX, nTargetY, nTargetZ = self:GetDesignator():GetTarget()
  local nDistance = nSpawnY - nTargetY - 15
  local nVectorX, nVectorY, nVectorZ = nTargetX - nSpawnX, nTargetY - nSpawnY, nTargetZ - nSpawnZ
  nVectorX, nVectorY, nVectorZ = Math.Normalize(nVectorX, nVectorY, nVectorZ)
  local nSpeedScale = 35
  self.uSpawnedBomb = Airstrike.SpawnOrdnance("Cluster Bomb Projectile", nSpawnX, nSpawnY, nSpawnZ, nVectorX * nSpeedScale, nVectorY * nSpeedScale, nVectorZ * nSpeedScale, "obstructed", 30, self:GetOwner(), BombExplodes, {self})
end

function BombExplodes(self)
  local nSpawnX, nSpawnY, nSpawnZ = Object.GetPosition(self.uSpawnedBomb)
  local nTargetX, nTargetY, nTargetZ, uGuid, uTarget = self:GetDesignator():GetTarget()
  local nSpeed = 20
  local nVectorX, nVectorY, nVectorZ = nTargetX - nSpawnX, nTargetY - nSpawnY, nTargetZ - nSpawnZ
  nVectorX, nVectorY, nVectorZ = Math.Normalize(nVectorX, nVectorY, nVectorZ)
  Airstrike.ConeSpawn("Cluster Bomblet Projectile", nSpawnX, nSpawnY, nSpawnZ, nVectorX, nVectorY, nVectorZ, 15, nSpeed, 10)
  Airstrike.ConeSpawn("Cluster Bomblet Projectile", nSpawnX, nSpawnY, nSpawnZ, nVectorX, nVectorY, nVectorZ, 30, nSpeed, 20)
end
