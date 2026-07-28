import("MrxUtil")
local deathCount = 0
local tDrops = {}
tDrops[0] = "Ammo Pickup (Bullet)"
tDrops[1] = "Ammo Pickup (Rocket)"
local tEvents = tEvents or {}

function OnDeath(uGuid, iArg)
  deathCount = deathCount + 1
  iArg = iArg or 0
  if Object.InSeat(uGuid) then
    return
  end
  local x, y, z = Object.GetPosition(uGuid)
  local bSpawned = false
  local sPickup = "Ammo Pickup (Small)"
  if Object.HasLabel(uGuid, "HeavySoldier") or deathCount / 3 == math.floor(deathCount / 3) then
    if Object.HasLabel(uGuid, "RocketSoldier") and not Object.HasLabel(uGuid, "MGSoldier") then
      sPickup = "Ammo Pickup (Rocket)"
    elseif Object.HasLabel(uGuid, "HeavySoldier") then
      sPickup = "Ammo Pickup (Bullet)"
    end
    for index, uPlayer in ipairs(Player.GetAllPlayers()) do
      local uHero = Player.GetCharacter(uPlayer)
      local tWeapons = Human.Inventory.GetAllWeapons(uHero)
      for i, weapon in pairs(tWeapons) do
        local iReserveAmmo = Weapon.GetReserveAmmo(weapon)
        if iReserveAmmo and Object.HasLabel(weapon, "Grenade") and iReserveAmmo < math.randf(8) then
          sPickup = "Ammo Pickup (Grenades)"
        end
      end
      if math.randf() * 80 > Object.GetHealth(uHero) then
        sPickup = "Health Pickup"
        bSpawned = true
      end
    end
    local uNewGuid = Pg.Spawn(sPickup, x, y + 0.1, z, 0, false, true, uGuid)
    if uNewGuid then
      Object.ApplyImpulse(uNewGuid, 0, -0.5, 0)
      Object.AddToDisposer(uNewGuid, "pickup")
    end
  end
end
