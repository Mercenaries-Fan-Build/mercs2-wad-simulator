inherit("MrxSupport")
import("MrxSupportDesignatorSatellite")
sBomb = "Smart Bomb Projectile"
uBomb = Pg.GetGuidByName("Smart Bomb Projectile")

function Create(oSelf, uPlayerGuid)
  local oNewSupport = {}
  setmetatable(oNewSupport, oSelf)
  oSelf.__index = oSelf
  local oDesignator = MrxSupportDesignatorSatellite:Create()
  oDesignator:SetCost(0)
  oNewSupport:SetDesignator(oDesignator)
  oDesignator:SetMinigameSectors({
    {45, 90},
    {152, 203},
    {270, 315}
  })
  oNewSupport:SetOwner(uPlayerGuid)
  oNewSupport:SetRecruit("Pilot")
  oNewSupport:SetModuleName("MrxSatelliteGuidedBomb")
  oNewSupport.sDeliveryVehicle = oSelf.sDeliveryVehicle
  oNewSupport.uDeliveryVehicle = Pg.GetGuidByName(oSelf.sDeliveryVehicle)
  oNewSupport.sBomb = oSelf.sBomb
  oNewSupport.uBomb = Pg.GetGuidByName(oSelf.sBomb)
  return oNewSupport
end

function DesignationCallback(oSelf)
  local nSpawnX, nSpawnY, nSpawnZ = Pg.FindPointFromCamera(-300, 8, -1, oSelf.uOwner)
  local nLaunchX, nLaunchY, nLaunchZ = Pg.FindPointFromCamera(-100, 80)
  local nTargetX, nTargetY, nTargetZ = oSelf:GetDesignator():GetTarget()
  local uJet = Airstrike.Flyby(oSelf.uDeliveryVehicle, nSpawnX, nSpawnZ, nLaunchX, nLaunchZ, nLaunchY, 200, DropBomb, {oSelf})
  oSelf.uJet = uJet
  MrxSupport.PlayAirstrikeVO(uJet, {
    "Misha-None-Freeplay-Support-01",
    "Misha-None-Freeplay-Support-10",
    "Misha-None-Freeplay-Support-21",
    "Misha-None-Freeplay-Support-24",
    "Misha-None-Freeplay-Support-28",
    "Misha-None-Freeplay-Support-32"
  })
end

function DropBomb(oSelf)
  local nSpawnX, nSpawnY, nSpawnZ = Object.GetPosition(oSelf.uJet)
  local nTargetX, nTargetY, nTargetZ = oSelf:GetDesignator():GetTarget()
  local nVectorX, nVectorY, nVectorZ = nTargetX - nSpawnX, nTargetY - nSpawnY, nTargetZ - nSpawnZ
  nVectorX, nVectorY, nVectorZ = Math.Normalize(nVectorX, nVectorY, nVectorZ)
  oSelf.uTarget = uGuid
  oSelf.tBombs = {}
  oSelf.tBombs[1] = Airstrike.SpawnOrdnance(oSelf.uBomb, nSpawnX, nSpawnY, nSpawnZ, nVectorX * 110, nVectorY * 110, nVectorZ * 110, "impact", 2, oSelf:GetOwner(), BombExplodes, {oSelf, 1})
end

function BombExplodes(oSelf, nIndex)
  local nBombX, nBombY, nBombZ = Object.GetPosition(oSelf.tBombs[nIndex])
  if nBombX and nBombY and nBombZ then
    Pg.Spawn("Explosion (Grenade)", nBombX, nBombY, nBombZ)
  end
end

function FinalExplosion(oSelf)
  local nBombX, nBombY, nBombZ = Object.GetPosition(oSelf.uSpawnedBomb)
  Pg.Spawn("Explosion (C4)", nBombX, nBombY, nBombZ)
end
