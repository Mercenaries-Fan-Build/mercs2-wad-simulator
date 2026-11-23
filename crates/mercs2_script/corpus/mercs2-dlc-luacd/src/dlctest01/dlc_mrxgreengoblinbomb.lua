inherit("MrxSupport", false)
import("MrxSupportDesignatorSatellite", false)
import("MrxSupportDesignatorSmoke", false)
tVOCues = {
  "Misha-None-Freeplay-Support-03",
  "Misha-None-Freeplay-Support-14",
  "Misha-None-Freeplay-Support-25"
}

function Create(oSelf, uPlayerGuid)
  local oNewSupport = {}
  setmetatable(oNewSupport, oSelf)
  oSelf.__index = oSelf
  local oDesignator = MrxSupportDesignatorSmoke:Create()
  oNewSupport:SetDesignator(oDesignator)
  oDesignator:SetAATestLevel("medium")
  oDesignator:SetValidationFunction(_NoValidation)
  oNewSupport:SetOwner(uPlayerGuid)
  oNewSupport:SetRecruit("Pilot")
  oNewSupport:SetDesignator(oDesignator)
  oNewSupport:SetModuleName("DLC_MrxGreenGoblinBomb")
  oNewSupport.sDeliveryVehicle = oSelf.sDeliveryVehicle
  return oNewSupport
end

function DesignationCallback(oSelf)
  local nSpawnX, nSpawnY, nSpawnZ = Pg.FindPointFromCamera(300, 200, -1, oSelf.uOwner)
  local nTargetX, nTargetY, nTargetZ = Pg.FindPointFromCamera(100, 50, -1, oSelf.uOwner)
  local uJet = Airstrike.Flyby(oSelf.sDeliveryVehicle, nSpawnX, nSpawnZ, nTargetX, nTargetZ, nTargetY, 100, DropBomb, {
    oSelf,
    nTargetX,
    nTargetY,
    nTargetZ
  })
  oSelf.uJet = uJet
  MrxSupport.PlayRandomVOCue(tVOCues)
  Event.Create(Event.TimerRelative, {1}, oSelf:GetDesignator():RemoveSmoke())
end

function DropBomb(oSelf, nTargetX, nTargetY, nTargetZ)
  local nSpawnX, nSpawnY, nSpawnZ = Object.GetPosition(oSelf.uJet)
  local nTargetX, nTargetY, nTargetZ = oSelf:GetDesignator():GetTarget()
  local nDistance = nSpawnY - nTargetY - 15
  local nVectorX, nVectorY, nVectorZ = nTargetX - nSpawnX, nTargetY - nSpawnY, nTargetZ - nSpawnZ
  nVectorX, nVectorY, nVectorZ = Math.Normalize(nVectorX, nVectorY, nVectorZ)
  local nSpeedScale = 35
  oSelf.uBomb = Airstrike.SpawnOrdnance("DLC Green Goblin Bomb Projectile", nSpawnX, nSpawnY, nSpawnZ, nVectorX * nSpeedScale, nVectorY * nSpeedScale, nVectorZ * nSpeedScale, "distance", nDistance, oSelf:GetOwner(), BombExplodes, {oSelf})
end

function BombExplodes(oSelf)
  local nSpawnX, nSpawnY, nSpawnZ = Object.GetPosition(oSelf.uBomb)
  local nTargetX, nTargetY, nTargetZ, uGuid, uTarget = oSelf:GetDesignator():GetTarget()
  local nSpeed = 20
  local nVectorX, nVectorY, nVectorZ = nTargetX - nSpawnX, nTargetY - nSpawnY, nTargetZ - nSpawnZ
  nVectorX, nVectorY, nVectorZ = Math.Normalize(nVectorX, nVectorY, nVectorZ)
  Airstrike.ConeSpawn("DLC Green Goblin Bomblet Projectile", nSpawnX, nSpawnY, nSpawnZ, nVectorX, nVectorY, nVectorZ, 15, nSpeed, 10)
  Airstrike.ConeSpawn("DLC Green Goblin Bomblet Projectile", nSpawnX, nSpawnY, nSpawnZ, nVectorX, nVectorY, nVectorZ, 30, nSpeed, 20)
end
