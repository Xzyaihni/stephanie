(load-once "infos.scm")

(define (destroy-this)
    ; make this spawn a bunch of star particles or something idk
    (remove-inventory-item caller-entity caller-item-inventory-id))
