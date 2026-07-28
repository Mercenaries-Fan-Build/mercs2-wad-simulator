inherit("MrxSupport")
import("MrxSupportDesignatorSatellite")
tVOCues = {
  "Misha-None-Freeplay-Support-03",
  "Misha-None-Freeplay-Support-14",
  "Misha-None-Freeplay-Support-25"
}

function Create(oSelf, uPlayerGuid)
  local oNewSupport = {}
  setmetatable(oNewSupport, oSelf)
  oSelf.__index = oSelf
  local oDesignator = MrxSupportDesignatorSatellite:Create()
  oNewSupport:SetDesignator(oDesignator)
  oDesignator:SetValidationFunction(_NoValidation)
  oNewSupport:SetOwner(uPlayerGuid)
  oNewSupport:SetRecruit("Pilot")
  oNewSupport:SetDesignator(oDesignator)
  oNewSupport:SetModuleName("MrxSatClusterBomb")
  oNewSupport.sDeliveryVehicle = oSelf.sDeliveryVehicle
  oNewSupport.uDeliveryVehicle = Pg.GetGuidByName(oSelf.sDeliveryVehicle)
  oDesignator:SetMinigameSectors({
    {45, 135},
    {225, 315}
  })
  oDesignator:SetCost(0)
  return oNewSupport
end

function DesignationCallback(oSelf)
  local nSpawnX, nSpawnY, nSpawnZ = Pg.FindPointFromCamera(300, 200, -1, oSelf.uOwner)
  local nTargetX, nTargetY, nTargetZ = Pg.FindPointFromCamera(100, 50, -1, oSelf.uOwner)
  local uJet = Airstrike.Flyby(oSelf.uDeliveryVehicle, nSpawnX, nSpawnZ, nTargetX, nTargetZ, nTargetY, 100, DropBomb, {
    oSelf,
    nTargetX,
    nTargetY,
    nTargetZ
  })
  oSelf.uJet = uJet
end

function DropBomb(oSelf, nTargetX, nTargetY, nTargetZ)
  local nSpawnX, nSpawnY, nSpawnZ = Object.GetPosition(oSelf.uJet)
  local nTargetX, nTargetY, nTargetZ = oSelf:GetDesignator():GetTarget()
  local nDistance = nSpawnY - nTargetY - 15
  local nVectorX, nVectorY, nVectorZ = nTargetX - nSpawnX, nTargetY - nSpawnY, nTargetZ - nSpawnZ
  nVectorX, nVectorY, nVectorZ = Math.Normalize(nVectorX, nVectorY, nVectorZ)
  local nSpeedScale = 35
  oSelf.uSpawnedBomb = Airstrike.SpawnOrdnance("Cluster Bomb Projectile", nSpawnX, nSpawnY, nSpawnZ, nVectorX * nSpeedScale, nVectorY * nSpeedScale, nVectorZ * nSpeedScale, "distance", nDistance, oSelf:GetOwner(), BombExplodes, {oSelf})
end

function BombExplodes(oSelf)
  local nSpawnX, nSpawnY, nSpawnZ = Object.GetPosition(oSelf.uSpawnedBomb)
  local nTargetX, nTargetY, nTargetZ, uGuid, uTarget = oSelf:GetDesignator():GetTarget()
  local nSpeed = 20
  local nVectorX, nVectorY, nVectorZ = nTargetX - nSpawnX, nTargetY - nSpawnY, nTargetZ - nSpawnZ
  nVectorX, nVectorY, nVectorZ = Math.Normalize(nVectorX, nVectorY, nVectorZ)
  Airstrike.ConeSpawn("Cluster Bomblet Projectile", nSpawnX, nSpawnY, nSpawnZ, nVectorX, nVectorY, nVectorZ, 10, nSpeed, 4)
  Airstrike.ConeSpawn("Cluster Bomblet Projectile", nSpawnX, nSpawnY, nSpawnZ, nVectorX, nVectorY, nVectorZ, 15, nSpeed, 4)
  Airstrike.ConeSpawn("Cluster Bomblet Projectile", nSpawnX, nSpawnY, nSpawnZ, nVectorX, nVectorY, nVectorZ, 30, nSpeed, 6)
  Airstrike.ConeSpawn("Cluster Bomblet Projectile", nSpawnX, nSpawnY, nSpawnZ, nVectorX, nVectorY, nVectorZ, 50, nSpeed, 6)
end
